//! The vault model: what notes exist, and what each one is.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::RelPath;
use crate::fs::{DirEntry, FileIdentity, Fs, Kind, Mtime, Result};

/// Guards against a pathological tree; no real vault approaches it.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(u32);

impl EntryId {
    /// No scan mints this: it names a file opened by path instead.
    pub const DETACHED: Self = Self(0);
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: EntryId,
    pub path: RelPath,
    pub kind: Kind,
    /// Unread; conflict detection compares a `Stamp` inside `Fs::save` instead.
    pub mtime: Mtime,
}

pub struct Vault {
    root: PathBuf,
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    /// Position of each id's row in `entries`. Rebuilt in `apply` from the
    /// final, sorted vector, never before the sort.
    by_id: HashMap<EntryId, usize>,
    next_id: u32,
}

impl Vault {
    /// Never returns the `Vec`: swapping it for a `SumTree` must stay internal.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.by_id.get(&id).map(|&index| &self.entries[index])
    }

    /// The id the last scan gave the file with this identity. Looked up by
    /// inode, never by path: a path lookup would answer with whatever holds
    /// that name *now*, which after an external move is a different file.
    #[must_use]
    pub fn id_for(&self, identity: FileIdentity) -> Option<EntryId> {
        self.by_identity.get(&identity).copied()
    }

    #[must_use]
    pub fn absolute(&self, id: EntryId) -> Option<PathBuf> {
        self.entry(id)
            .map(|entry| self.root.join(entry.path.as_path()))
    }

    /// Re-keys an entry after a write replaced the file's inode. A no-op when
    /// `id` names no live entry: a `rescan` racing this call may already have
    /// minted a fresh id, and re-keying on top would leave `by_identity` and
    /// `entries` disagreeing about which id the path carries.
    pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity) {
        if !self.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        self.by_identity.retain(|_, mapped| *mapped != id);
        self.by_identity.insert(identity, id);
    }

    /// The walk owns only the depth limit; filtering entries is `Fs`'s job.
    /// Iterative because an async recursive function needs boxing per level.
    ///
    /// # Errors
    ///
    /// Never, today. The `Result` is the seam a second `Fs` backend will need.
    pub async fn scan(fs: &dyn Fs, root: &Path) -> Result<Self> {
        let mut vault = Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            by_identity: HashMap::new(),
            by_id: HashMap::new(),
            next_id: 1,
        };
        vault.rescan(fs).await?;
        Ok(vault)
    }

    /// Ids are keyed by `(device, inode)`, so a rename keeps its id and a
    /// recreated file takes a new one.
    ///
    /// # Errors
    ///
    /// Never, today. See `scan`.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let children = Self::collect(fs, &self.root).await?;
        self.apply(children);
        Ok(())
    }

    /// Everything on disk under `root`, flattened, depth-limited. Split from
    /// `apply` because it awaits and `apply` needs `&mut self`, which an
    /// `Entity<Vault>` only hands out synchronously.
    ///
    /// # Errors
    ///
    /// When the root cannot be read. A subdirectory that fails is skipped
    /// instead, so one folder cannot lose the vault — but an unreadable root
    /// would otherwise clear every entry and strand every open note.
    pub async fn collect(fs: &dyn Fs, root: &Path) -> Result<Vec<DirEntry>> {
        let mut found = Vec::new();
        let mut worklist = vec![(root.to_path_buf(), 0usize)];

        while let Some((dir, depth)) = worklist.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let children = match fs.read_dir(&dir).await {
                Ok(children) => children,
                Err(error) if dir == root => return Err(error),
                Err(_) => continue,
            };

            for child in children {
                if child.kind == Kind::Dir {
                    worklist.push((child.path.clone(), depth + 1));
                }
                found.push(child);
            }
        }
        Ok(found)
    }

    /// Mints and reuses ids for what `collect` found, keyed by
    /// `(device, inode)`.
    pub fn apply(&mut self, children: Vec<DirEntry>) {
        let mut found: Vec<Entry> = Vec::new();
        let mut next_by_identity = HashMap::new();

        for child in children {
            let Ok(relative) = child.path.strip_prefix(&self.root) else {
                continue;
            };
            let Some(path) = RelPath::new(relative) else {
                continue;
            };

            // Hard links collapse, first path wins.
            if next_by_identity.contains_key(&child.identity) {
                continue;
            }
            let id = if let Some(existing) = self.by_identity.get(&child.identity) {
                *existing
            } else {
                let id = EntryId(self.next_id);
                self.next_id += 1;
                id
            };
            next_by_identity.insert(child.identity, id);

            found.push(Entry {
                id,
                path,
                kind: child.kind,
                mtime: child.mtime,
            });
        }

        found.sort_by(compare);

        self.by_id = found
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id, index))
            .collect();
        self.by_identity = next_by_identity;
        self.entries = found;
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Directories precede files at each level, and a directory is followed by
/// its whole subtree. Two simpler encodings were tried and rejected: raw path
/// bytes wedge `Projects.md` between `Projects` and `Projects/plan.md`, and
/// `(kind, path)` hoists every directory above every file, breaking subtree
/// contiguity in both cases.
fn compare(a: &Entry, b: &Entry) -> Ordering {
    let a_components: Vec<_> = a.path.as_path().components().collect();
    let b_components: Vec<_> = b.path.as_path().components().collect();

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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::expect_used))]
mod tests;
