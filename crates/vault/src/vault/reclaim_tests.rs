//! `Vault::id_at` and `Vault::reclaim` (watcher spec, F9).

use std::collections::BTreeSet;
use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::{assert_consistent, id_of, paths, rel, scan};
use super::*;
use crate::mem_fs::MemFs;

/// No scan minted it: the id of a note gone from disk.
const GONE: EntryId = EntryId(900);

fn tree() -> (MemFs, Vault) {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/f/b.md", "b");
    fs.insert_dir("/vault/f/g");
    fs.insert_dir("/vault/x.md");
    fs.insert_file("/vault/z.md", "z");
    let vault = scan(&fs);
    (fs, vault)
}

#[test]
fn id_at_finds_a_note_at_its_path() {
    let (_fs, vault) = tree();

    for path in ["a.md", "f/b.md", "z.md"] {
        assert_eq!(vault.id_at(&rel(path)), id_of(&vault, path), "{path}");
        assert!(vault.id_at(&rel(path)).is_some(), "{path}");
    }
}

#[test]
fn id_at_finds_nothing_at_a_missing_path_or_a_folder() {
    let (_fs, vault) = tree();

    for path in ["b.md", "f/a.md", "f/g", "f", "x.md", "f/g/c.md"] {
        assert_eq!(vault.id_at(&rel(path)), None, "{path}");
    }
}

#[test]
fn reclaim_gives_the_row_the_notes_id_and_its_inode() {
    let (fs, mut vault) = tree();
    let other = id_of(&vault, "f/b.md").expect("b");
    let identity = block_on(fs.load(Path::new("/vault/f/b.md")))
        .expect("load")
        .identity;
    let before = paths(&vault);

    assert!(vault.reclaim(GONE, other));

    assert_eq!(id_of(&vault, "f/b.md"), Some(GONE));
    assert_eq!(vault.entry(other).map(|e| e.id), None);
    assert_eq!(
        vault.entry(GONE).map(|e| e.path.clone()),
        Some(rel("f/b.md"))
    );
    assert_eq!(vault.id_for(identity), Some(GONE));
    assert_eq!(vault.id_at(&rel("f/b.md")), Some(GONE));
    assert_eq!(paths(&vault), before);
}

#[test]
fn reclaim_refuses_a_folder_an_absent_row_or_a_live_id() {
    let (_fs, vault) = tree();
    let folder = id_of(&vault, "f/g").expect("g");
    let a = id_of(&vault, "a.md").expect("a");
    let z = id_of(&vault, "z.md").expect("z");

    for (id, other) in [
        (GONE, folder),
        (GONE, EntryId(901)),
        (a, z),
        (EntryId::DETACHED, z),
    ] {
        let mut reclaimed = vault.clone();
        assert!(!reclaimed.reclaim(id, other), "{id:?} from {other:?}");
        let ids = |vault: &Vault| vault.entries().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(&reclaimed), ids(&vault));
        assert_eq!(
            reclaimed.entry(other).map(|e| e.id),
            vault.entry(other).map(|e| e.id)
        );
    }
}

/// A scoped read of `folder`, applied.
fn read(vault: &Vault, fs: &MemFs, folder: &str) -> Vault {
    let folders = BTreeSet::from([rel(folder)]);
    vault.applied(block_on(vault.walk_changed(fs, &folders, &Serial)))
}

fn identity(fs: &MemFs, path: &str) -> FileIdentity {
    block_on(fs.load(Path::new(path))).expect("load").identity
}

/// `f/a.md` gone from disk while its folder is read, then a new note there, reclaimed by the gone id.
fn reclaimed() -> (MemFs, Vault, EntryId, FileIdentity) {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/a.md", "a");
    fs.insert_file("/vault/g/b.md", "b");
    fs.insert_dir("/outside");
    let vault = scan(&fs);
    let a = id_of(&vault, "f/a.md").expect("a");
    let gone = identity(&fs, "/vault/f/a.md");
    block_on(fs.rename(Path::new("/vault/f/a.md"), Path::new("/outside/a.md"))).expect("move out");
    let vault = read(&vault, &fs, "f");
    assert_eq!(vault.entry(a).map(|e| e.id), None, "gone");
    fs.insert_file("/vault/f/a.md", "new");
    let mut vault = read(&vault, &fs, "f");
    let other = id_of(&vault, "f/a.md").expect("the new note");
    assert_ne!(other, a, "the gone inode is not found, not replaced");
    assert!(vault.reclaim(a, other));
    (fs, vault, a, gone)
}

#[test]
fn a_reclaimed_id_answers_to_the_new_inode_alone() {
    let (fs, vault, a, gone) = reclaimed();

    let vault = read(&vault, &fs, "g");

    assert_consistent(&vault);
    assert_eq!(id_of(&vault, "f/a.md"), Some(a));
    assert_eq!(vault.id_for(identity(&fs, "/vault/f/a.md")), Some(a));
    assert_eq!(vault.id_for(gone), None);
}

#[test]
fn the_gone_inode_moved_back_takes_a_new_id() {
    let (fs, vault, a, gone) = reclaimed();
    block_on(fs.rename(Path::new("/outside/a.md"), Path::new("/vault/g/a.md"))).expect("move in");

    let vault = read(&vault, &fs, "g");

    assert_consistent(&vault);
    assert_eq!(id_of(&vault, "f/a.md"), Some(a));
    let back = vault.id_for(gone).expect("found");
    assert_ne!(back, a);
    assert_eq!(id_of(&vault, "g/a.md"), Some(back));
}
