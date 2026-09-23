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
mod tests {
    use std::path::Path;

    use futures::executor::block_on;

    use super::*;
    use crate::mem_fs::MemFs;

    fn scan(fs: &MemFs) -> Vault {
        block_on(Vault::scan(fs, Path::new("/vault"))).expect("scan")
    }

    fn ordered_paths(fs: &MemFs) -> Vec<String> {
        scan(fs)
            .entries()
            .map(|e| e.path.as_path().to_string_lossy().into_owned())
            .collect()
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

        block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
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

    #[test]
    fn a_save_keeps_the_entry_id_across_a_rescan() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let mut vault = scan(&fs);
        let id = vault.entries().next().expect("one entry").id;

        let stamp = block_on(fs.load(Path::new("/vault/a.md")))
            .expect("load")
            .stamp;
        let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
        vault.reidentify(id, saved.identity);
        block_on(vault.rescan(&fs)).expect("rescan");

        assert_eq!(
            vault.entry(id).map(|e| e.path.display_name()).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn a_save_without_reidentify_loses_the_entry_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let mut vault = scan(&fs);
        let id = vault.entries().next().expect("one entry").id;

        let stamp = block_on(fs.load(Path::new("/vault/a.md")))
            .expect("load")
            .stamp;
        block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
        block_on(vault.rescan(&fs)).expect("rescan");

        assert!(vault.entry(id).is_none(), "the old id names nothing");
    }

    #[test]
    fn reidentify_ignores_an_entry_a_rescan_already_replaced() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let mut vault = scan(&fs);
        let stale = vault.entries().next().expect("one entry").id;

        let stamp = block_on(fs.load(Path::new("/vault/a.md")))
            .expect("load")
            .stamp;
        let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
        block_on(vault.rescan(&fs)).expect("rescan");
        let minted = vault.entries().next().expect("one entry").id;
        vault.reidentify(stale, saved.identity);
        block_on(vault.rescan(&fs)).expect("rescan");

        assert_eq!(
            vault.entries().next().map(|e| e.id),
            Some(minted),
            "the id the rescan committed is the one that survives"
        );
    }

    #[test]
    fn the_scan_never_mints_the_detached_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/sub/b.md", "b");

        assert!(scan(&fs).entries().all(|e| e.id != EntryId::DETACHED));
    }

    #[test]
    fn entry_resolves_every_id_after_a_rescan_reorders_the_list() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a/c.md", "c");
        fs.insert_file("/vault/b.md", "b");

        let vault = scan(&fs);

        for entry in vault.entries() {
            let found = vault
                .entry(entry.id)
                .expect("every id the scan minted must resolve");
            assert_eq!(found.id, entry.id, "entry() returned the wrong row");
            assert_eq!(
                found.path, entry.path,
                "entry() returned a row for a different path"
            );
        }
    }

    #[test]
    fn a_folder_note_does_not_split_its_folder_from_its_children() {
        let fs = MemFs::new();
        fs.insert_file("/vault/Projects/plan.md", "p");
        fs.insert_file("/vault/Projects.md", "n");

        assert_eq!(
            ordered_paths(&fs),
            vec!["Projects", "Projects/plan.md", "Projects.md"]
        );
    }

    #[test]
    fn a_directorys_children_follow_it_rather_than_all_other_directories() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a/z.md", "z");
        fs.insert_file("/vault/c/y.md", "y");
        fs.insert_file("/vault/b.md", "b");

        assert_eq!(
            ordered_paths(&fs),
            vec!["a", "a/z.md", "c", "c/y.md", "b.md"]
        );
    }

    /// The invariant the sidebar's collapse-skip rests on.
    #[test]
    fn every_directorys_descendants_are_contiguous() {
        let fs = MemFs::new();
        fs.insert_file("/vault/Projects/plan.md", "p");
        fs.insert_file("/vault/Projects/deep/x.md", "x");
        fs.insert_file("/vault/Projects.md", "n");
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/zz/other.md", "o");

        let vault = scan(&fs);
        let entries: Vec<_> = vault.entries().collect();

        for (i, entry) in entries.iter().enumerate() {
            if entry.kind != Kind::Dir {
                continue;
            }
            let descendants = entries
                .iter()
                .filter(|c| c.path != entry.path && c.path.starts_with(&entry.path))
                .count();
            for offset in 1..=descendants {
                assert!(
                    entries[i + offset].path.starts_with(&entry.path),
                    "{:?} is not contiguous: index {} breaks the run",
                    entry.path.as_path(),
                    i + offset
                );
            }
        }
    }
}
