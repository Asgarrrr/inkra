//! `walk` then `applied` against today's `apply`, and a partial walk against a full one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use proptest::prelude::*;

use super::*;
use crate::fs::DirEntry;
use crate::mem_fs::MemFs;

const ROOT: &str = "/vault";

/// Today's `apply`, over input sorted first: the slice-6 minting rule, written the old way.
struct Reference {
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    next_id: u32,
}

impl Reference {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_identity: HashMap::new(),
            next_id: 1,
        }
    }

    /// `reidentify` without the reclaim: a no-op for a dead id, with the same mtime rule (O2).
    fn reidentify(&mut self, id: EntryId, identity: FileIdentity, mtime: Mtime) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        if self
            .by_identity
            .get(&identity)
            .is_none_or(|minted| *minted == id)
        {
            entry.mtime = mtime;
        }
        self.by_identity.retain(|_, mapped| *mapped != id);
        self.by_identity.insert(identity, id);
    }

    /// The sort is the slice-6 rule (ids in path order), the path's id for a replaced inode is
    /// the watcher spec's F5; the rest is today's `apply`, verbatim.
    fn apply(&mut self, mut children: Vec<DirEntry>) {
        let root = Path::new(ROOT);
        children.sort_by(|a, b| compare(&Key::of(&probe(root, a)), &Key::of(&probe(root, b))));
        let claimed: HashSet<EntryId> = children
            .iter()
            .filter_map(|child| self.by_identity.get(&child.identity).copied())
            .collect();
        let mut found = Vec::new();
        let mut next_by_identity = HashMap::new();
        for child in children {
            let Some(path) = child.path.strip_prefix(root).ok().and_then(RelPath::new) else {
                continue;
            };
            if next_by_identity.contains_key(&child.identity) {
                continue;
            }
            let id = if let Some(existing) = self.by_identity.get(&child.identity) {
                *existing
            } else if let Some(replaced) = self.entries.iter().find(|entry| {
                entry.path == path && entry.kind == child.kind && !claimed.contains(&entry.id)
            }) {
                replaced.id
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
        found.sort_by(|a, b| compare(&Key::of(a), &Key::of(b)));
        self.by_identity = next_by_identity;
        self.entries = found;
    }
}

fn probe(root: &Path, child: &DirEntry) -> Entry {
    let path = child
        .path
        .strip_prefix(root)
        .ok()
        .and_then(RelPath::new)
        .unwrap_or_else(RelPath::root);
    Entry {
        id: EntryId::DETACHED,
        path,
        kind: child.kind,
        mtime: child.mtime,
    }
}

pub(super) type Rows = Vec<(EntryId, PathBuf, Kind, Mtime)>;

pub(super) fn rows<'a>(entries: impl Iterator<Item = &'a Entry>) -> Rows {
    entries
        .map(|e| (e.id, e.path.as_path().to_path_buf(), e.kind, e.mtime))
        .collect()
}

#[derive(Debug, Clone)]
pub(super) enum Op {
    Note {
        folder: usize,
        name: usize,
    },
    Folder {
        folder: usize,
        name: usize,
    },
    Remove(usize),
    Rename {
        pick: usize,
        name: usize,
    },
    Recreate(usize),
    HardLink {
        pick: usize,
        folder: usize,
        name: usize,
    },
    Save(usize),
    /// A save that keeps the inode, as `RealFs` writes a hard-linked or symlinked note.
    SaveInPlace(usize),
}

const NAMES: &[&str] = &["a", "b", "Ab", "é", "z"];

/// Every op but a hard link.
pub(super) fn edit() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..6, 0..NAMES.len()).prop_map(|(folder, name)| Op::Note { folder, name }),
        (0usize..6, 0..NAMES.len()).prop_map(|(folder, name)| Op::Folder { folder, name }),
        (0usize..12).prop_map(Op::Remove),
        (0usize..12, 0..NAMES.len()).prop_map(|(pick, name)| Op::Rename { pick, name }),
        (0usize..12).prop_map(Op::Recreate),
        (0usize..12).prop_map(Op::Save),
        (0usize..12).prop_map(Op::SaveInPlace),
    ]
}

fn hard_link() -> impl Strategy<Value = Op> {
    (0usize..12, 0usize..6, 0..NAMES.len()).prop_map(|(pick, folder, name)| Op::HardLink {
        pick,
        folder,
        name,
    })
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![6 => edit(), 1 => hard_link()]
}

/// Folders an op may target: the root, then every folder `under` holds, in vault order.
fn folders(vault: &Vault, under: &Path) -> Vec<PathBuf> {
    let mut all = vec![Path::new(ROOT).join(under)];
    all.extend(
        vault
            .entries()
            .filter(|e| {
                e.kind == Kind::Dir
                    && e.path.as_path().starts_with(under)
                    && e.path.as_path() != under
            })
            .map(|e| Path::new(ROOT).join(e.path.as_path())),
    );
    all
}

/// Notes and folders an op may move or remove, `under` excluded.
fn picks(vault: &Vault, under: &Path) -> Vec<(EntryId, PathBuf, Kind)> {
    vault
        .entries()
        .filter(|e| e.path.as_path().starts_with(under) && e.path.as_path() != under)
        .map(|e| (e.id, Path::new(ROOT).join(e.path.as_path()), e.kind))
        .collect()
}

/// Runs `op` on `fs`, confined to `under`; a save re-keys through `rekey`.
pub(super) fn run(
    fs: &MemFs,
    vault: &Vault,
    under: &Path,
    op: &Op,
    rekey: &mut dyn FnMut(EntryId, FileIdentity, Mtime),
) {
    let folders = folders(vault, under);
    let picks = picks(vault, under);
    let exists =
        |path: &Path| fs.read_to_string(path).is_some() || block_on(fs.read_dir(path)).is_ok();
    match *op {
        Op::Note { folder, name } => {
            let path = folders[folder % folders.len()].join(format!("{}.md", NAMES[name]));
            if !exists(&path) {
                fs.insert_file(path, "new");
            }
        }
        Op::Folder { folder, name } => {
            let path = folders[folder % folders.len()].join(NAMES[name]);
            if !exists(&path) {
                fs.insert_dir(path);
            }
        }
        Op::Remove(pick) => {
            if let Some((_, path, _)) = picks.get(pick) {
                fs.remove(path);
            }
        }
        Op::Rename { pick, name } => {
            if let Some((_, from, kind)) = picks.get(pick) {
                let file = match kind {
                    Kind::Note => format!("{}.md", NAMES[name]),
                    Kind::Dir => NAMES[name].to_owned(),
                };
                let to = from.with_file_name(file);
                let _ = block_on(fs.rename(from, &to));
            }
        }
        Op::Recreate(pick) => {
            if let Some((_, path, Kind::Note)) = picks.get(pick) {
                fs.remove(path);
                fs.insert_file(path.clone(), "again");
            }
        }
        Op::HardLink { pick, folder, name } => {
            let to = folders[folder % folders.len()].join(format!("{}.md", NAMES[name]));
            if let Some((_, from, Kind::Note)) = picks.get(pick)
                && !exists(&to)
            {
                fs.hard_link(from, to);
            }
        }
        Op::Save(pick) => {
            if let Some((id, path, Kind::Note)) = picks.get(pick)
                && let Ok(loaded) = block_on(fs.load(path))
                && let Ok(saved) = block_on(fs.save(path, "written", loaded.stamp))
            {
                rekey(*id, saved.identity, saved.stamp.mtime);
            }
        }
        Op::SaveInPlace(pick) => {
            if let Some((id, path, Kind::Note)) = picks.get(pick) {
                fs.insert_file(path.clone(), "in place");
                if let Ok(saved) = block_on(fs.load(path)) {
                    rekey(*id, saved.identity, saved.stamp.mtime);
                }
            }
        }
    }
}

/// A move, or a hard link, of one entry across the boundary of a partial walk.
#[derive(Debug, Clone)]
struct Cross {
    pick: usize,
    folder: usize,
    name: usize,
    link: bool,
}

fn cross() -> impl Strategy<Value = Cross> {
    (
        0usize..12,
        0usize..6,
        0..NAMES.len(),
        prop::bool::weighted(0.25),
    )
        .prop_map(|(pick, folder, name, link)| Cross {
            pick,
            folder,
            name,
            link,
        })
}

/// Moves or links an entry below `under` out of it, or one outside it in; the path it moved or linked out.
fn run_across(fs: &MemFs, vault: &Vault, under: &Path, cross: &Cross) -> Option<PathBuf> {
    // `under` and its parents stay: the partial walk starts at `under`.
    let picks: Vec<&Entry> = vault
        .entries()
        .filter(|e| !under.starts_with(e.path.as_path()))
        .collect();
    let entry = picks.get(cross.pick)?;
    let from = Path::new(ROOT).join(entry.path.as_path());
    let leaving = entry.path.as_path().starts_with(under);
    let mut folders: Vec<PathBuf> = vec![PathBuf::new()];
    folders.extend(
        vault
            .entries()
            .filter(|e| e.kind == Kind::Dir)
            .map(|e| e.path.as_path().to_path_buf()),
    );
    folders.retain(|folder| folder.starts_with(under) != leaving);
    if folders.is_empty() {
        return None;
    }
    let folder = Path::new(ROOT).join(&folders[cross.folder % folders.len()]);
    let to = match entry.kind {
        Kind::Note => folder.join(format!("{}.md", NAMES[cross.name])),
        Kind::Dir => folder.join(NAMES[cross.name]),
    };
    let exists = fs.read_to_string(&to).is_some() || block_on(fs.read_dir(&to)).is_ok();
    if cross.link {
        if entry.kind == Kind::Note && !exists {
            fs.hard_link(&from, to);
            return leaving.then_some(from);
        }
        None
    } else {
        let moved = block_on(fs.rename(&from, &to)).is_ok();
        (moved && leaving).then_some(from)
    }
}

/// The inode at `path`, as its folder lists it.
fn identity_at(fs: &MemFs, path: &Path) -> Option<FileIdentity> {
    let listing = block_on(fs.read_dir(path.parent()?)).ok()?;
    listing
        .into_iter()
        .find(|child| child.path == path)
        .map(|child| child.identity)
}

pub(super) fn walk(fs: &MemFs, from: &Path) -> Walk {
    let from = RelPath::new(from).expect("relative");
    block_on(Vault::walk(fs, Path::new(ROOT), &from, &Serial)).expect("the start reads")
}

proptest! {
    #[test]
    fn a_full_walk_applied_matches_todays_apply(ops in prop::collection::vec(op(), 1..30)) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        let mut reference = Reference::new();
        for op in &ops {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
                reference.reidentify(id, identity, mtime);
            }
            // A walk overwrites every mtime a re-key set: compare before it.
            prop_assert_eq!(rows(vault.entries()), rows(reference.entries.iter()));
            let found = walk(&fs, Path::new(""));
            reference.apply(found.found.clone());
            vault = vault.applied(found);

            prop_assert_eq!(rows(vault.entries()), rows(reference.entries.iter()));
            prop_assert_eq!(vault.state.next_id, reference.next_id);
            prop_assert_eq!(&vault.state.by_identity, &reference.by_identity);
            for (at, entry) in vault.entries().enumerate() {
                prop_assert_eq!(vault.state.by_id.get(&entry.id), Some(&at));
            }
        }
    }

    #[test]
    fn a_partial_walk_matches_a_full_one(
        // No hard link in the setup: one spanning the boundary of `under` is shown by the path the walk read.
        setup in prop::collection::vec(edit(), 1..20),
        pick in 0usize..8,
        inside in prop::collection::vec(op(), 1..12),
    ) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
            }
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        let mut dirs: Vec<PathBuf> = vec![PathBuf::new()];
        dirs.extend(vault.entries().filter(|e| e.kind == Kind::Dir).map(|e| e.path.as_path().to_path_buf()));
        let under = dirs[pick % dirs.len()].clone();

        // Every change stays below `under`: a walk from it reads all of them.
        for op in &inside {
            let mut rekeys = Vec::new();
            run(&fs, &vault, &under, op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
            }
        }
        let partial = vault.applied(walk(&fs, &under));
        let full = vault.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(rows(partial.entries()), rows(full.entries()));
        prop_assert_eq!(partial.state.next_id, full.state.next_id);
        // The partial walk also keeps the identities it no longer sees; its live ones match.
        let live: HashMap<FileIdentity, EntryId> = partial
            .state
            .by_identity
            .iter()
            .filter(|(_, id)| partial.state.by_id.contains_key(id))
            .map(|(identity, id)| (*identity, *id))
            .collect();
        prop_assert_eq!(&live, &full.state.by_identity);
    }

    #[test]
    fn a_partial_walk_then_a_full_one_matches_a_full_one(
        setup in prop::collection::vec(edit(), 1..20),
        pick in 0usize..8,
        across in prop::collection::vec(cross(), 1..8),
        inside in prop::collection::vec(op(), 0..12),
    ) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
            }
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        let mut dirs: Vec<PathBuf> = vec![PathBuf::new()];
        dirs.extend(vault.entries().filter(|e| e.kind == Kind::Dir).map(|e| e.path.as_path().to_path_buf()));
        let under = dirs[pick % dirs.len()].clone();

        let left: Vec<(PathBuf, Option<FileIdentity>)> = across
            .iter()
            .filter_map(|cross| run_across(&fs, &vault, &under, cross))
            .map(|from| {
                let identity = identity_at(&fs, &from);
                (from, identity)
            })
            .collect();
        // New files only below `under`: a partial walk mints first what it reads, not in path order.
        for op in &inside {
            run(&fs, &vault, &under, op, &mut |_, _, _| {});
        }
        // An inode moved or linked out, then a new one at its path, read in two walks: the watcher spec's F5 limit.
        prop_assume!(left.iter().all(|(path, identity)| identity_at(&fs, path) == *identity));
        let settled = vault.applied(walk(&fs, &under)).applied(walk(&fs, Path::new("")));
        let full = vault.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(rows(settled.entries()), rows(full.entries()));
        prop_assert_eq!(settled.state.next_id, full.state.next_id);
    }
    /// A walk from the root answers for every identity, even when a folder fails (O3).
    #[test]
    fn a_root_walk_that_fails_a_folder_keeps_its_entries_and_no_dead_identity(
        setup in prop::collection::vec(edit(), 1..20),
        pick in 0usize..8,
        after in prop::collection::vec(op(), 0..12),
    ) {
        let fs = MemFs::new();
        // A folder with a note from the start, so most cases have one to fail.
        fs.insert_file(Path::new(ROOT).join("f/a.md"), "");
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
            }
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        let dirs: Vec<(EntryId, PathBuf)> = vault
            .entries()
            .filter(|e| e.kind == Kind::Dir)
            .map(|e| (e.id, e.path.as_path().to_path_buf()))
            .collect();
        prop_assume!(!dirs.is_empty());
        let (failed_id, failed) = dirs[pick % dirs.len()].clone();
        fs.make_unreadable(Path::new(ROOT).join(&failed));
        for op in &after {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity, mtime| rekeys.push((id, identity, mtime)));
            for (id, identity, mtime) in rekeys {
                let _ = vault.reidentify(id, identity, mtime);
            }
        }
        let before = vault.clone();
        let scanned = vault.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(scanned.state.by_identity.len(), scanned.entries().count());
        for id in scanned.state.by_identity.values() {
            prop_assert!(scanned.state.by_id.contains_key(id), "{:?} names no entry", id);
        }
        // The root walk found the same folder where it was, and could not read it.
        if scanned.entry(failed_id).is_some_and(|e| e.path.as_path() == failed) {
            // An entry a hard link's re-key left with no identity may go (O3).
            let identified: HashSet<EntryId> = before.state.by_identity.values().copied().collect();
            for kept in before
                .entries()
                .filter(|e| e.path.as_path().starts_with(&failed) && identified.contains(&e.id))
            {
                let now = scanned.entry(kept.id);
                prop_assert!(now.is_some(), "{:?} lost its entry", kept.path.as_path());
                // Unless the walk found it elsewhere, it stays where it was.
                let now = now.map(|e| e.path.as_path());
                prop_assert!(
                    now == Some(kept.path.as_path()) || !now.is_some_and(|p| p.starts_with(&failed)),
                    "{:?} moved to {:?} inside the folder it could not read", kept.path.as_path(), now
                );
            }
        }
    }

    /// The full pass continues the root pass's listing: the same result as a walk that lists the root again.
    #[test]
    fn a_walk_seeded_with_the_root_listing_matches_a_full_walk(
        setup in prop::collection::vec(op(), 1..20),
        after in prop::collection::vec(op(), 0..12),
    ) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            run(&fs, &vault, Path::new(""), op, &mut |_, _, _| {});
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        for op in &after {
            run(&fs, &vault, Path::new(""), op, &mut |_, _, _| {});
        }
        let root = Path::new(ROOT);
        let listing = Walk {
            read: vec![RelPath::root()],
            found: block_on(fs.read_dir(root)).expect("the root reads"),
            reach: Reach::Partial,
        };
        let after_root = vault.applied(listing.clone());
        let listed = fs.read_dirs(root);

        let seeded = after_root.applied(block_on(Vault::walk_seeded(&fs, root, listing, &Serial)));
        prop_assert_eq!(fs.read_dirs(root), listed, "the seeded walk lists the root no more");
        let full = after_root.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(rows(seeded.entries()), rows(full.entries()));
        prop_assert_eq!(seeded.state.next_id, full.state.next_id);
        prop_assert_eq!(&seeded.state.by_identity, &full.state.by_identity, "complete: it drops what a full walk drops");
    }
}
