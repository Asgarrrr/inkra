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
    /// The scan mints ids from 1, so this names a note no scan can produce: a
    /// file opened by path. A test holds the reservation.
    pub const DETACHED: Self = Self(0);
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: EntryId,
    pub path: RelPath,
    pub kind: Kind,
    /// Unread: conflict detection compares a `Stamp` inside `Fs::save`, never
    /// this. Kept because a listing that cannot report an mtime is a poorer
    /// seam for a second `Fs` backend, not because anything currently reads it.
    pub mtime: Mtime,
}

pub struct Vault {
    root: PathBuf,
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    /// Position of each id's row in `entries`. Rebuilt in `apply`, from the
    /// final vector — the only place `entries` is replaced. Built before the
    /// `sort_by(compare)` there, it would point every id at its pre-sort row.
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

    /// The id the last scan gave the file with this identity.
    ///
    /// How a caller that has just created a file finds the row for it: by
    /// inode, never by the path it asked for. A path lookup would answer with
    /// whatever holds that name *now*, which after an external move is a
    /// different file — and the caller would select a row for a note it did
    /// not create.
    ///
    /// The opposite question — what id a path has, for a `[[link]]` with no
    /// identity in hand — is `by_path`, still deferred.
    #[must_use]
    pub fn id_for(&self, identity: FileIdentity) -> Option<EntryId> {
        self.by_identity.get(&identity).copied()
    }

    #[must_use]
    pub fn absolute(&self, id: EntryId) -> Option<PathBuf> {
        self.entry(id)
            .map(|entry| self.root.join(entry.path.as_path()))
    }

    /// Re-key an entry after a write replaced the file. `RealFs::save` renames a
    /// temp file over the destination, so a saved note's `(device, inode)` is
    /// not the one it was scanned with. Without this the next `rescan` mints a
    /// fresh `EntryId`, and every caller keyed by the old one holds a note that
    /// no longer exists.
    ///
    /// A no-op when `id` names no live entry: a `rescan` landing between the
    /// write and this call has already minted that fresh id and committed it,
    /// so re-keying on top would leave `by_identity` and `entries` disagreeing
    /// about which id the path carries, and a click on the row would open a
    /// second buffer on the same file.
    pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity) {
        if !self.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        // By value, not by key: `by_identity` maps identity → id, and the
        // identity we are replacing is precisely the one we no longer know.
        self.by_identity.retain(|_, mapped| *mapped != id);
        self.by_identity.insert(identity, id);
    }

    /// The walk owns the depth limit and nothing else. Which entries exist —
    /// `*.md` only, no dotfiles, no symlinks — is `Fs`'s obligation (see
    /// `RealFs::read_dir`), so a second backend must reimplement those filters.
    /// Iterative because an async recursive function needs boxing per level.
    ///
    /// # Errors
    ///
    /// Never, today: an unreadable directory is skipped rather than reported,
    /// so one folder cannot lose the vault. The `Result` is the seam a second
    /// `Fs` backend will need.
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

    /// Everything on disk under `root`, flattened, depth-limited.
    ///
    /// Split from `apply` because it awaits and `apply` needs `&mut self`: an
    /// `Entity<Vault>` hands out `&mut Vault` only for the body of a
    /// synchronous closure, so a caller holding the entity runs this half
    /// first and gives the result to the other.
    ///
    /// A directory is descended into before `apply`'s filters have run on it.
    /// Unreachable in practice: POSIX forbids a hard link to a directory, so
    /// the collapse never fires on one, and `strip_prefix` cannot fail for a
    /// child of the root the walk descended from.
    ///
    /// # Errors
    ///
    /// When the **root** cannot be read. A subdirectory that cannot be read is
    /// skipped instead, so one folder cannot lose the vault — but the root is
    /// not one folder among many. Reporting it as an empty listing would make
    /// `apply` clear every entry, and a caller that rescans a live vault would
    /// then leave every open note unable to resolve its path: still on screen,
    /// still typable, never written again, not even by the quit flush. The
    /// startup path already treats an unreadable root as fatal
    /// (`app/src/main.rs`), and this is the same fact arriving later.
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

    /// Mints and reuses ids for what `collect` found. Ids are keyed by
    /// `(device, inode)`, so a rename keeps its id and a recreated file takes a
    /// new one.
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

            // Hard links collapse, first path wins. Before minting, so the
            // duplicate neither burns an id nor overwrites the survivor's
            // map entry — either shifts the survivor's id next rescan.
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

        // From the final, sorted vector: `entries` and `by_id` must agree on
        // each id's row, and the sort is what decides the row.
        self.by_id = found
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id, index))
            .collect();
        self.by_identity = next_by_identity;
        self.entries = found;
    }

    /// Where the vault starts. `app` needs it to drive `collect` itself.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Directories precede files at each level; a directory is followed by its
/// whole subtree. Only the first half is this function's doing — contiguity
/// comes from `RelPath`'s component-wise `Ord` and survives `path.cmp`.
/// Sibling order prefers whichever entry is a directory *at the first differing
/// component*, which is not the same as the entry being a directory.
///
/// Two encodings were rejected and must not return as simplifications; both
/// lose contiguity. Raw path *bytes* wedge `Projects.md` between `Projects` and
/// `Projects/plan.md`, since `.` (0x2E) sorts below `/` (0x2F). `(kind, path)`
/// hoists every directory above every file, landing an unrelated top-level
/// directory inside another directory's run.
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
    // One is an ancestor of the other: the ancestor comes first.
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

    /// Dropping the duplicate must not leave the identity map pointing at an id
    /// no entry carries: the next rescan would hand it to the survivor.
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

    /// `reidentify` is what keeps the id across a save, and with it whatever
    /// buffer a caller holds under it.
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

    /// The failure `reidentify` exists to prevent, pinned so removing the call
    /// fails here rather than silently orphaning an open buffer.
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

    /// The race `reidentify`'s no-op guard exists for — see `Vault::reidentify`.
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

    /// `EntryId::DETACHED` is safe only while no scan can mint it.
    #[test]
    fn the_scan_never_mints_the_detached_id() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "a");
        fs.insert_file("/vault/sub/b.md", "b");

        assert!(scan(&fs).entries().all(|e| e.id != EntryId::DETACHED));
    }

    /// `apply` mints ids while walking `collect`'s discovery order, then
    /// reorders the whole vector with `sort_by(compare)` — an id minted early
    /// can land at a late row. Pins that the lookup index is built from the
    /// sorted vector, not pre-sort positions, which would otherwise map an id
    /// to the wrong row.
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

    /// Pins component-wise ordering, not `compare`: holds under a plain
    /// `path.cmp`, but byte ordering breaks it, and the Obsidian folder-note
    /// pattern reaches that on the first real vault.
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

    /// Under `(kind, path)`, `a/z.md` would render indented under `c`.
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

    /// The invariant the sidebar's collapse-skip rests on. Cannot discriminate
    /// `compare` from `path.cmp`, but both rejected encodings break it: byte
    /// order wedges `Projects.md` at index 1, `(kind, path)` wedges `zz` at
    /// index 2. The second top-level directory is what gives that case teeth.
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
