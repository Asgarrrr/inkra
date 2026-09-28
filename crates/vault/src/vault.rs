//! The vault model: what notes exist, and what each one is.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::RelPath;
use crate::fs::{FileIdentity, Fs, Kind, Mtime, Result};

mod identity;
mod reclaim;
mod walk;

pub use walk::{Job, Jobs, Reach, Serial, Walk};

/// Guards against a pathological tree; no real vault approaches it.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(u32);

impl EntryId {
    /// No scan mints this: it names a file opened by path instead.
    pub const DETACHED: Self = Self(0);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    pub path: RelPath,
    pub kind: Kind,
    /// What `IndexSync` compares to reload a note; conflict detection compares a `Stamp`.
    pub mtime: Mtime,
}

/// Cloning is an `Arc` bump: a scan applies onto a clone off the foreground (open-first spec, O1).
/// Equality compares every entry and identity: off the foreground at 50,000 notes.
#[derive(Clone, PartialEq, Eq)]
pub struct Vault {
    state: Arc<State>,
}

#[derive(Clone, PartialEq, Eq)]
struct State {
    root: PathBuf,
    entries: Arc<[Entry]>,
    by_identity: HashMap<FileIdentity, EntryId>,
    /// Position of each id's row in `entries`, rebuilt from the final, sorted list.
    by_id: HashMap<EntryId, usize>,
    next_id: u32,
    /// A full scan has applied; never turns back.
    scanned: bool,
    /// A `Reach::Whole` walk has applied; never turns back. Unlike `scanned`, O8 does not set it.
    read_whole: bool,
}

impl Vault {
    /// Holds nothing until a scan applies: the first frame reads no folder.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            state: Arc::new(State {
                root: root.to_path_buf(),
                entries: Arc::from([]),
                by_identity: HashMap::new(),
                by_id: HashMap::new(),
                next_id: 1,
                scanned: false,
                read_whole: false,
            }),
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.state.entries.iter()
    }

    /// The entries for a background task, without a copy.
    #[must_use]
    pub fn shared_entries(&self) -> Arc<[Entry]> {
        Arc::clone(&self.state.entries)
    }

    #[must_use]
    pub fn is_scanned(&self) -> bool {
        self.state.scanned
    }

    /// Whether some walk has read every folder: until then, a scoped read has no base (watcher spec, F4).
    #[must_use]
    pub(crate) fn has_read_whole(&self) -> bool {
        self.state.read_whole
    }

    pub(crate) fn mark_scanned(&mut self) {
        if !self.state.scanned {
            Arc::make_mut(&mut self.state).scanned = true;
        }
    }

    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.state
            .by_id
            .get(&id)
            .map(|&index| &self.state.entries[index])
    }

    /// The id a scan gave this identity; it may name no live entry (open-first spec, O3).
    #[must_use]
    pub fn id_for(&self, identity: FileIdentity) -> Option<EntryId> {
        self.state.by_identity.get(&identity).copied()
    }

    #[must_use]
    pub fn absolute(&self, id: EntryId) -> Option<PathBuf> {
        self.entry(id)
            .map(|entry| self.state.root.join(entry.path.as_path()))
    }

    /// Re-keys `id` after a save; whether an entry's id moved (open-first spec, O2).
    #[must_use]
    pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity, mtime: Mtime) -> bool {
        if id == EntryId::DETACHED {
            return false;
        }
        let live = self.state.by_id.get(&id).copied();
        let minted = self.state.by_identity.get(&identity).copied();
        if live.is_none() && minted.is_none() {
            return false;
        }
        let state = Arc::make_mut(&mut self.state);
        // A partial walk may have kept the replaced inode on `id`.
        state.by_identity.retain(|_, mapped| *mapped != id);
        state.by_identity.insert(identity, id);
        // A mapping a partial walk kept, with no entry: the next scan gives the file `id`.
        let Some(at) = live.or_else(|| {
            let at = state.by_id.remove(&minted?)?;
            state.by_id.insert(id, at);
            Some(at)
        }) else {
            return false;
        };
        // A walk that minted this inode read it at or after the save, maybe after a write in place.
        let saved = minted.is_none_or(|minted| minted == id);
        let entry = &state.entries[at];
        // A build may share the entries: copying them is foreground time.
        if entry.id != id || (saved && entry.mtime != mtime) {
            let entry = &mut Arc::make_mut(&mut state.entries)[at];
            entry.id = id;
            if saved {
                entry.mtime = mtime;
            }
        }
        live.is_none()
    }

    /// `reidentify` at a scan's swap. `in_place`: the live vault mapped `identity` to `id` when the re-key was queued.
    #[must_use]
    pub(crate) fn replay_reidentify(
        &mut self,
        id: EntryId,
        identity: FileIdentity,
        mtime: Mtime,
        in_place: bool,
    ) -> bool {
        // A moved inode the walk mapped to `id` was read at or after the save; a kept one maybe before (watcher spec, F5).
        if !in_place && self.id_for(identity) == Some(id) {
            return false;
        }
        self.reidentify(id, identity, mtime)
    }

    /// # Errors
    ///
    /// When the root cannot be read.
    pub async fn scan(fs: &dyn Fs, root: &Path) -> Result<Self> {
        let mut vault = Self::new(root);
        vault.rescan(fs).await?;
        Ok(vault)
    }

    /// A full scan: a rename keeps its id, a recreated file takes a new one.
    ///
    /// # Errors
    ///
    /// When the root cannot be read; the vault is then unchanged.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let root = self.state.root.clone();
        let walk = Self::walk(fs, &root, &RelPath::root(), &Serial).await?;
        *self = self.applied(walk);
        self.mark_scanned();
        Ok(())
    }

    /// Replaces the children of every folder `walk` read and keeps the rest (open-first spec, O3).
    #[must_use]
    pub fn applied(&self, walk: Walk) -> Self {
        let state = &self.state;
        let whole = walk.reach == Reach::Whole;
        let read: HashSet<&Path> = walk.read.iter().map(RelPath::as_path).collect();
        let mut rows: Vec<(Entry, FileIdentity)> = walk
            .found
            .into_iter()
            .filter_map(|child| {
                let path = RelPath::new(child.path.strip_prefix(&state.root).ok()?)?;
                let entry = Entry {
                    id: EntryId::DETACHED,
                    path,
                    kind: child.kind,
                    mtime: child.mtime,
                };
                Some((entry, child.identity))
            })
            .collect();
        // A file the walk found claims its inode from an entry kept under a folder it did not read.
        let unclaimed: HashSet<FileIdentity> = rows.iter().map(|(_, identity)| *identity).collect();
        for (identity, id) in &state.by_identity {
            if unclaimed.contains(identity) {
                continue;
            }
            let Some(entry) = self.entry(*id) else {
                continue;
            };
            if entry
                .path
                .as_path()
                .parent()
                .is_some_and(|parent| read.contains(parent))
            {
                continue;
            }
            rows.push((entry.clone(), *identity));
        }
        // Path order, not the walk's: a parallel walk reads folders in no fixed order.
        let mut rows = sorted(rows);
        let next_id = self.mint(&mut rows, unclaimed);

        // A folder sorts before its subtree, so one pass knows every surviving folder.
        let keep: Vec<bool> = {
            let mut folders: HashSet<&Path> = HashSet::new();
            rows.iter()
                .map(|(entry, _)| {
                    let path = entry.path.as_path();
                    let kept = path.parent().is_none_or(|parent| {
                        parent.as_os_str().is_empty() || folders.contains(parent)
                    });
                    if kept && entry.kind == Kind::Dir {
                        folders.insert(path);
                    }
                    kept
                })
                .collect()
        };
        let mut by_identity = HashMap::with_capacity(rows.len());
        let mut entries = Vec::with_capacity(rows.len());
        for ((entry, identity), kept) in rows.into_iter().zip(keep) {
            if kept {
                by_identity.insert(identity, entry.id);
                entries.push(entry);
            }
        }
        let by_id: HashMap<EntryId, usize> = entries
            .iter()
            .enumerate()
            .map(|(at, entry)| (entry.id, at))
            .collect();
        // A whole walk answers for every identity: what it keeps unread stays an entry.
        if !whole {
            self.carry_over(&mut by_identity, &by_id);
        }
        Self {
            state: Arc::new(State {
                root: state.root.clone(),
                entries: entries.into(),
                by_identity,
                by_id,
                next_id,
                scanned: state.scanned,
                read_whole: state.read_whole || whole,
            }),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.state.root
    }
}

/// Directories first at each level, each followed by its subtree (vault spec, "Ordering").
fn compare(a: &Key<'_>, b: &Key<'_>) -> Ordering {
    let (a_components, b_components) = (&a.components, &b.components);
    for (i, (x, y)) in a_components.iter().zip(b_components.iter()).enumerate() {
        if x == y {
            continue;
        }
        let x_is_dir = i + 1 < a_components.len() || a.kind == Kind::Dir;
        let y_is_dir = i + 1 < b_components.len() || b.kind == Kind::Dir;
        return match (x_is_dir, y_is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => x.cmp(y),
        };
    }
    // An ancestor of the other: the ancestor comes first.
    a_components.len().cmp(&b_components.len())
}

/// A path split once, not once per comparison.
struct Key<'a> {
    components: Vec<Component<'a>>,
    kind: Kind,
}

impl<'a> Key<'a> {
    fn of(entry: &'a Entry) -> Self {
        Self {
            components: entry.path.as_path().components().collect(),
            kind: entry.kind,
        }
    }
}

fn sorted(rows: Vec<(Entry, FileIdentity)>) -> Vec<(Entry, FileIdentity)> {
    let order: Vec<usize> = {
        let keys: Vec<Key<'_>> = rows.iter().map(|(entry, _)| Key::of(entry)).collect();
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&a, &b| compare(&keys[a], &keys[b]));
        order
    };
    let mut rows: Vec<Option<(Entry, FileIdentity)>> = rows.into_iter().map(Some).collect();
    order.into_iter().filter_map(|at| rows[at].take()).collect()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
pub(crate) mod test_helpers;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod walk_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod parallel_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod oracle_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod changed_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod replay_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod reclaim_tests;
