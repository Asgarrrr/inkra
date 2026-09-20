//! The vault model: what notes exist, and what each one is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::RelPath;
use crate::fs::{FileIdentity, Fs, Kind, Mtime, Result};

/// No real vault approaches this. It exists so a pathological tree cannot
/// exhaust the walk.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(u32);

impl EntryId {
    /// Reserved: the scan mints ids from 1, so this one names a note no scan
    /// can produce — a file opened by path with no vault behind it, which is
    /// what `app` launches with when it is given a filename on the command
    /// line. A test below holds the reservation.
    pub const DETACHED: Self = Self(0);
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: EntryId,
    pub path: RelPath,
    pub kind: Kind,
    pub mtime: Mtime,
}

pub struct Vault {
    root: PathBuf,
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    next_id: u32,
}

impl Vault {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Never returns the `Vec`. Swapping the container for a `SumTree` the day
    /// a profile asks for one must stay internal.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The absolute path to an entry, for handing to `Fs`.
    #[must_use]
    pub fn absolute(&self, id: EntryId) -> Option<PathBuf> {
        self.entry(id)
            .map(|entry| self.root.join(entry.path.as_path()))
    }

    /// Walks the tree from `root`, keeping `*.md` and every directory.
    ///
    /// Iterative rather than recursive: an async recursive function needs
    /// boxing at every level, and an explicit worklist gives the depth limit
    /// for free.
    ///
    /// # Errors
    ///
    /// Never, today: a directory that cannot be read is skipped rather than
    /// reported, so that one unreadable folder does not lose the vault. The
    /// `Result` is the seam a second `Fs` backend will need.
    pub async fn scan(fs: &dyn Fs, root: &Path) -> Result<Self> {
        let mut vault = Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            by_identity: HashMap::new(),
            next_id: 1,
        };
        vault.rescan(fs).await?;
        Ok(vault)
    }

    /// Re-walks the tree, preserving the id of every file whose identity is
    /// unchanged. Ids are keyed by `(device, inode)`, so a rename keeps its id
    /// and a recreated file takes a new one.
    ///
    /// # Errors
    ///
    /// Never, today. See `scan`.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let mut found: Vec<Entry> = Vec::new();
        let mut next_by_identity = HashMap::new();
        let mut worklist = vec![(self.root.clone(), 0usize)];

        while let Some((dir, depth)) = worklist.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            // One unreadable folder must not lose the vault.
            let Ok(children) = fs.read_dir(&dir).await else {
                continue;
            };

            for child in children {
                let Ok(relative) = child.path.strip_prefix(&self.root) else {
                    continue;
                };
                let Some(path) = RelPath::new(relative) else {
                    continue;
                };

                // Hard links collapse: the first path wins, the second is
                // dropped. A note vault does not use them. Checked before an
                // id is minted, so the duplicate neither burns an id nor
                // overwrites the survivor's entry in the map — that would hand
                // the survivor a different id on the next rescan.
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

                if child.kind == Kind::Dir {
                    worklist.push((child.path.clone(), depth + 1));
                }
                found.push(Entry {
                    id,
                    path,
                    kind: child.kind,
                    mtime: child.mtime,
                });
            }
        }

        self.by_identity = next_by_identity;
        self.entries = found;
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::expect_used))]
mod tests {
    use std::path::Path;

    use futures::executor::block_on;

    use super::*;
    use crate::mem_fs::MemFs;

    fn scan(fs: &MemFs) -> Vault {
        block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
    }

    #[test]
    fn the_scan_finds_notes_in_subdirectories() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/sub/b.md", "b");

        let vault = scan(&fs);

        let names: Vec<_> = vault.entries().map(|e| e.path.display_name()).collect();
        assert!(names.contains(&"a".to_owned()));
        assert!(names.contains(&"b".to_owned()));
    }

    #[test]
    fn an_empty_directory_is_kept_as_an_entry() {
        let fs = MemFs::new();
        fs.insert_dir("/vault/empty");

        let vault = scan(&fs);

        assert_eq!(vault.entries().count(), 1);
        assert_eq!(vault.entries().next().map(|e| e.kind), Some(Kind::Dir));
    }

    #[test]
    fn an_unreadable_subdirectory_does_not_lose_the_vault() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_dir("/vault/locked");
        fs.make_unreadable("/vault/locked");

        let vault = scan(&fs);

        assert!(vault.entries().any(|e| e.path.display_name() == "a"));
    }

    #[test]
    fn a_rename_keeps_the_entry_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        let mut vault = scan(&fs);
        let id = vault.entries().next().expect("one entry").id;

        fs.rename(Path::new("/vault/a.md"), "/vault/b.md");
        block_on(vault.rescan(&fs)).expect("rescan");

        let entry = vault.entry(id).expect("the id must survive a rename");
        assert_eq!(entry.path.display_name(), "b");
    }

    #[test]
    fn a_file_recreated_at_the_same_path_gets_a_new_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "first");
        let mut vault = scan(&fs);
        let old = vault.entries().next().expect("one entry").id;

        fs.remove(Path::new("/vault/a.md"));
        fs.insert_file("/vault/a.md", "second");
        block_on(vault.rescan(&fs)).expect("rescan");

        let new = vault.entries().next().expect("one entry").id;
        assert_ne!(old, new, "a different file must not inherit the id");
    }

    /// Two names for one file collapse to one entry, and the walk that drops
    /// the duplicate must not leave the identity map pointing at an id no
    /// entry carries — the next rescan would hand the survivor that id.
    #[test]
    fn a_hard_link_does_not_shift_the_surviving_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.hard_link(Path::new("/vault/a.md"), "/vault/b.md");
        let mut vault = scan(&fs);
        let before: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
        assert_eq!(before.len(), 1, "one file, one entry");

        block_on(vault.rescan(&fs)).expect("rescan");

        let after: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
        assert_eq!(
            before, after,
            "nothing changed on disk, so no id may change"
        );
    }

    /// `EntryId::DETACHED` is only safe to hand to a note outside the vault for
    /// as long as no scan can mint it.
    #[test]
    fn the_scan_never_mints_the_detached_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/sub/b.md", "b");

        assert!(scan(&fs).entries().all(|e| e.id != EntryId::DETACHED));
    }
}
