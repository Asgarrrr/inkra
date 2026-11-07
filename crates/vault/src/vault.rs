//! The vault model: what notes exist, and what each one is.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::RelPath;
use crate::fs::{DirEntry, FileIdentity, Fs, Kind, Mtime, Result, Stamp};
use crate::jobs::Serial;

mod departures;
mod identity;
mod names;
mod order;
mod reclaim;
mod walk;

pub use departures::Departures;
use order::sorted;
pub use walk::{Reach, Walk};

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
    pub mtime: Mtime,
    pub size: u64,
    pub identity: FileIdentity,
}

impl Entry {
    /// A note's stem, a folder's or an asset's whole name. Lossy rather than fallible: a filename
    /// that is not valid UTF-8 should render with replacement characters, not vanish from the vault.
    #[must_use]
    pub fn display_name(&self) -> String {
        let path = self.path.as_path();
        let name = match self.kind {
            Kind::Note => path.file_stem(),
            Kind::Dir | Kind::Asset => path.file_name(),
        };
        name.map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }

    /// What `IndexSync` compares to reload a note, as conflict detection does.
    #[must_use]
    pub fn stamp(&self) -> Stamp {
        Stamp {
            mtime: self.mtime,
            size: self.size,
        }
    }
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
    departures: Departures,
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
                departures: Departures::default(),
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
    pub fn has_read_whole(&self) -> bool {
        self.state.read_whole
    }

    pub(crate) fn mark_scanned(&mut self) {
        if !self.state.scanned {
            Arc::make_mut(&mut self.state).scanned = true;
        }
    }

    #[must_use]
    pub fn departures(&self) -> &Departures {
        &self.state.departures
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
    pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity, stamp: Stamp) -> bool {
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
            state.departures.renew();
            Some(at)
        }) else {
            return false;
        };
        // A walk that minted this inode read it at or after the save, maybe after a write in place.
        let saved = minted.is_none_or(|minted| minted == id);
        let entry = &state.entries[at];
        // A build may share the entries: copying them is foreground time.
        if entry.id != id || entry.identity != identity || (saved && entry.stamp() != stamp) {
            let entry = &mut Arc::make_mut(&mut state.entries)[at];
            entry.id = id;
            entry.identity = identity;
            if saved {
                entry.mtime = stamp.mtime;
                entry.size = stamp.size;
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
        stamp: Stamp,
        in_place: bool,
    ) -> bool {
        // A moved inode the walk mapped to `id` was read at or after the save; a kept one maybe before (watcher spec, F5).
        if !in_place && self.id_for(identity) == Some(id) {
            return false;
        }
        self.reidentify(id, identity, stamp)
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
        let (rows, unclaimed) = self.collect_rows(walk.found, &read);
        // Path order, not the walk's: a parallel walk reads folders in no fixed order.
        let (rows, next_id) = self.mint(sorted(rows), unclaimed);
        let keep = surviving_rows(&rows);
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
        let departures = if state.by_id.keys().any(|id| !by_id.contains_key(id)) {
            Departures::default()
        } else {
            state.departures.clone()
        };
        Self {
            state: Arc::new(State {
                root: state.root.clone(),
                entries: entries.into(),
                by_identity,
                by_id,
                next_id,
                scanned: state.scanned,
                read_whole: state.read_whole || whole,
                departures,
            }),
        }
    }

    /// The rows a walk answers with (what it found, and the entries kept under a folder it did not
    /// read), and the found files' identities.
    fn collect_rows(
        &self,
        found: Vec<DirEntry>,
        read: &HashSet<&Path>,
    ) -> (Vec<Row>, HashSet<FileIdentity>) {
        let mut rows: Vec<Row> = found
            .into_iter()
            .filter_map(|child| {
                Some(Row {
                    id: None,
                    path: RelPath::new(child.path.strip_prefix(&self.state.root).ok()?)?,
                    kind: child.kind,
                    mtime: child.mtime,
                    size: child.size,
                    identity: child.identity,
                })
            })
            .collect();
        // A file the walk found claims its inode from an entry kept under a folder it did not read.
        let unclaimed: HashSet<FileIdentity> = rows.iter().map(|row| row.identity).collect();
        for (identity, id) in &self.state.by_identity {
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
            rows.push(Row {
                id: Some(entry.id),
                path: entry.path.clone(),
                kind: entry.kind,
                mtime: entry.mtime,
                size: entry.size,
                identity: *identity,
            });
        }
        (rows, unclaimed)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.state.root
    }
}

/// A row of a walk being applied: a kept entry has its id, a found file has `None` until minted.
struct Row {
    id: Option<EntryId>,
    path: RelPath,
    kind: Kind,
    mtime: Mtime,
    size: u64,
    identity: FileIdentity,
}

/// Whether each row's parent folder survives; a folder sorts before its subtree, so one pass knows.
fn surviving_rows(rows: &[(Entry, FileIdentity)]) -> Vec<bool> {
    let mut folders: HashSet<&Path> = HashSet::new();
    rows.iter()
        .map(|(entry, _)| {
            let path = entry.path.as_path();
            let kept = path
                .parent()
                .is_none_or(|parent| parent.as_os_str().is_empty() || folders.contains(parent));
            if kept && entry.kind == Kind::Dir {
                folders.insert(path);
            }
            kept
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod walk_tests;

#[cfg(test)]
mod parallel_tests;

#[cfg(test)]
mod oracle_tests;

#[cfg(test)]
mod changed_tests;

#[cfg(test)]
mod replay_tests;

#[cfg(test)]
mod reclaim_tests;

#[cfg(test)]
mod names_tests;

#[cfg(test)]
mod departures_tests;

#[cfg(test)]
mod order_tests;
