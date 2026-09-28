//! `walk_changed`: the folders a watcher named, and the new folders below them (watcher spec, F5).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use proptest::prelude::*;

use super::oracle_tests::{Op, edit, rows, run, walk};
use super::test_helpers::{id_of, rel, scan};
use super::*;
use crate::Pending;
use crate::mem_fs::MemFs;

const ROOT: &str = "/vault";

fn changed(vault: &Vault, fs: &MemFs, folders: &[&str]) -> Walk {
    let folders: BTreeSet<RelPath> = folders.iter().map(|folder| rel(folder)).collect();
    block_on(vault.walk_changed(fs, &folders, &Serial))
}

fn read(walk: &Walk) -> Vec<String> {
    let mut read: Vec<String> = walk
        .read
        .iter()
        .map(|folder| folder.as_path().to_string_lossy().into_owned())
        .collect();
    read.sort();
    read
}

#[test]
fn a_pending_folder_is_read_without_the_folders_the_vault_holds_below_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/c.md", "");
    let vault = scan(&fs);
    fs.insert_file("/vault/f/b.md", "");

    let walk = changed(&vault, &fs, &["f"]);

    assert_eq!(read(&walk), ["f"]);
    assert_eq!(
        fs.read_dirs(Path::new("/vault/f/g")),
        1,
        "the scan's read only"
    );
    let vault = vault.applied(walk);
    assert!(id_of(&vault, "f/b.md").is_some());
    assert!(id_of(&vault, "f/g/c.md").is_some(), "kept, unread");
}

#[test]
fn a_new_folder_below_a_pending_one_is_walked_whole() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/f");
    let vault = scan(&fs);
    fs.insert_file("/vault/f/new/deep/x.md", "");

    let walk = changed(&vault, &fs, &["f"]);

    assert_eq!(read(&walk), ["f", "f/new", "f/new/deep"]);
    assert!(id_of(&vault.applied(walk), "f/new/deep/x.md").is_some());
}

#[test]
fn a_pending_folder_that_is_new_is_read_once() {
    let fs = MemFs::new();
    fs.insert_dir("/vault/f");
    let vault = scan(&fs);
    fs.insert_file("/vault/f/new/x.md", "");

    let walk = changed(&vault, &fs, &["f", "f/new"]);

    assert_eq!(read(&walk), ["f", "f/new"]);
    assert_eq!(fs.read_dirs(Path::new("/vault/f/new")), 1);
}

#[test]
fn a_folder_renamed_is_walked_at_its_new_path_and_keeps_its_ids() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/sub/c.md", "");
    let vault = scan(&fs);
    let sub = id_of(&vault, "f/g/sub").expect("sub");
    let c = id_of(&vault, "f/g/sub/c.md").expect("c");
    block_on(fs.rename(Path::new("/vault/f/g"), Path::new("/vault/f/h"))).expect("rename");

    let walk = changed(&vault, &fs, &["f", "f/g", "f/h"]);

    assert_eq!(read(&walk), ["f", "f/h", "f/h/sub"], "`f/g` is gone");
    let vault = vault.applied(walk);
    assert_eq!(id_of(&vault, "f/h/sub"), Some(sub));
    assert_eq!(id_of(&vault, "f/h/sub/c.md"), Some(c));
    assert_eq!(id_of(&vault, "f/g"), None);
}

/// A folder replaced by another at its path: what it holds is another folder's.
#[test]
fn a_folder_replaced_at_its_path_is_walked_whole() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/sub/c.md", "");
    let vault = scan(&fs);
    fs.remove(Path::new("/vault/f/g"));
    fs.insert_file("/vault/f/g/sub/c.md", "");

    let walk = changed(&vault, &fs, &["f"]);

    assert_eq!(read(&walk), ["f", "f/g", "f/g/sub"]);
}

#[test]
fn a_pure_rename_swap_keeps_each_id_with_its_inode() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/a.md", "a");
    fs.insert_file("/vault/f/b.md", "b");
    let vault = scan(&fs);
    let a = id_of(&vault, "f/a.md").expect("a");
    let b = id_of(&vault, "f/b.md").expect("b");
    let rename = |from: &str, to: &str| {
        block_on(fs.rename(Path::new(from), Path::new(to))).expect("rename");
    };
    rename("/vault/f/a.md", "/vault/f/tmp");
    rename("/vault/f/b.md", "/vault/f/a.md");
    rename("/vault/f/tmp", "/vault/f/b.md");

    let vault = vault.applied(changed(&vault, &fs, &["f"]));

    assert_eq!(id_of(&vault, "f/b.md"), Some(a));
    assert_eq!(id_of(&vault, "f/a.md"), Some(b));
}

/// The note's inode is found elsewhere in the walk, so the file at its old path is another (F5).
#[test]
fn a_new_file_where_a_note_was_renamed_away_gets_a_new_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/a.md", "first");
    let vault = scan(&fs);
    let old = id_of(&vault, "f/a.md").expect("a");
    block_on(fs.rename(Path::new("/vault/f/a.md"), Path::new("/vault/f/b.md"))).expect("rename");
    fs.insert_file("/vault/f/a.md", "second");

    let vault = vault.applied(changed(&vault, &fs, &["f"]));

    assert_eq!(id_of(&vault, "f/b.md"), Some(old));
    let new = id_of(&vault, "f/a.md").expect("a");
    assert_ne!(new, old, "a different file must not inherit the id");
}

/// Reading the root's listing is not a whole walk: the note's inode outlives the first half (F5).
#[test]
fn a_note_moved_from_the_root_into_a_folder_across_two_batches_keeps_its_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/note.md", "");
    fs.insert_dir("/vault/sub");
    let vault = scan(&fs);
    let note = id_of(&vault, "note.md").expect("note");
    block_on(fs.rename(Path::new("/vault/note.md"), Path::new("/vault/sub/note.md")))
        .expect("rename");

    let vault = vault.applied(changed(&vault, &fs, &[""]));
    let vault = vault.applied(changed(&vault, &fs, &["sub"]));

    assert_eq!(id_of(&vault, "sub/note.md"), Some(note));
}

#[test]
fn a_pending_folder_that_no_longer_exists_is_skipped_and_its_parent_drops_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/c.md", "");
    let vault = scan(&fs);
    fs.remove(Path::new("/vault/f/g"));

    let walk = changed(&vault, &fs, &["f", "f/g"]);

    assert_eq!(read(&walk), ["f"]);
    assert_eq!(walk.reach, Reach::Partial, "a folder gone needs no retry");
    let vault = vault.applied(walk);
    assert_eq!(id_of(&vault, "f/g"), None);
    assert_eq!(id_of(&vault, "f/g/c.md"), None);
}

#[test]
fn a_pending_folder_replaced_by_a_file_is_skipped_and_its_parent_drops_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/c.md", "");
    let vault = scan(&fs);
    fs.remove(Path::new("/vault/f/g"));
    fs.insert_file("/vault/f/g", "");

    let walk = changed(&vault, &fs, &["f", "f/g"]);

    assert_eq!(read(&walk), ["f"]);
    assert_eq!(walk.reach, Reach::Partial, "a folder gone needs no retry");
    let vault = vault.applied(walk);
    assert_eq!(id_of(&vault, "f/g"), None);
    assert_eq!(id_of(&vault, "f/g/c.md"), None);
}

#[test]
fn a_pending_folder_that_fails_keeps_its_entries() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    let vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");
    fs.make_unreadable("/vault/f");

    let walk = changed(&vault, &fs, &["f"]);

    assert!(walk.read.is_empty());
    assert_eq!(walk.reach, Reach::Incomplete, "a full scan retries it (F5)");
    assert_eq!(id_of(&vault.applied(walk), "f/b.md"), Some(b));
}

/// As a full walk: a folder past the depth limit is listed by its parent, never read.
#[test]
fn a_pending_folder_past_the_depth_limit_is_not_read() {
    let fs = MemFs::new();
    let deepest: PathBuf = (0..=MAX_DEPTH).map(|_| "d").collect();
    fs.insert_file(Path::new(ROOT).join(&deepest).join("x.md"), "");
    let vault = scan(&fs);
    let last = deepest.to_string_lossy().into_owned();
    let limit = deepest
        .parent()
        .expect("a parent")
        .to_string_lossy()
        .into_owned();

    let walk = changed(&vault, &fs, &[&limit, &last]);

    assert_eq!(read(&walk), [limit]);
}

/// Runs `op`, then re-keys the vault after each save, as the app does.
fn run_and_reidentify(fs: &MemFs, vault: &mut Vault, op: &Op) {
    let mut rekeys = Vec::new();
    run(fs, vault, Path::new(""), op, &mut |id, identity, mtime| {
        rekeys.push((id, identity, mtime));
    });
    for (id, identity, mtime) in rekeys {
        let _ = vault.reidentify(id, identity, mtime);
    }
}

proptest! {
    /// Oracle: on the same vault, a scoped walk of the folders F4 derives from the watch's
    /// events applies to exactly what a full walk does — rows with their ids, the next id,
    /// and the identities of live entries. A full walk also forgets the identities it no
    /// longer sees; a partial one keeps them, so those are left out. `edit()` has no hard link,
    /// which a partial walk shows at the path it read (`oracle_tests`).
    #[test]
    fn a_scoped_walk_of_the_changed_folders_matches_a_full_walk(
        setup in prop::collection::vec(edit(), 1..20),
        after in prop::collection::vec(edit(), 1..12),
    ) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            run_and_reidentify(&fs, &mut vault, op);
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        let mut watch = fs.watch(Path::new(ROOT)).expect("watch");

        for op in &after {
            run_and_reidentify(&fs, &mut vault, op);
        }
        let mut pending = Pending::default();
        while let Ok(event) = watch.events.try_recv() {
            if let Some(changed) = watch.changed(&event) {
                pending.add(changed);
            }
        }
        let scoped = match pending {
            Pending::Folders(folders) => vault.applied(block_on(vault.walk_changed(&fs, &folders, &Serial))),
            Pending::Everything => vault.applied(walk(&fs, Path::new(""))),
        };
        let full = vault.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(rows(scoped.entries()), rows(full.entries()));
        prop_assert_eq!(scoped.state.next_id, full.state.next_id);
        let live: HashMap<FileIdentity, EntryId> = scoped
            .state
            .by_identity
            .iter()
            .filter(|(_, id)| scoped.state.by_id.contains_key(id))
            .map(|(identity, id)| (*identity, *id))
            .collect();
        prop_assert_eq!(&live, &full.state.by_identity);
    }
}
