use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::{paths, scan};
use super::*;
use crate::mem_fs::MemFs;

fn ordered_paths(fs: &MemFs) -> Vec<String> {
    paths(&scan(fs))
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

/// The note's inode is found elsewhere, so the file at its old path is another (watcher spec, F5).
#[test]
fn a_new_file_where_a_note_was_renamed_away_gets_a_new_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "first");
    let mut vault = scan(&fs);
    let old = vault.entries().next().expect("one entry").id;

    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    fs.insert_file("/vault/a.md", "second");
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(
        vault.entry(old).map(|e| e.path.display_name()).as_deref(),
        Some("b")
    );
    let new = vault
        .entries()
        .find(|e| e.path.display_name() == "a")
        .expect("a")
        .id;
    assert_ne!(new, old, "a different file must not inherit the id");
}

/// A kind changed at a path is another thing, whatever became of the old inode.
#[test]
fn a_folder_where_a_note_was_gets_a_new_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "note");
    let mut vault = scan(&fs);
    let old = vault.entries().next().expect("one entry").id;

    fs.remove(Path::new("/vault/a.md"));
    fs.insert_dir("/vault/a.md");
    block_on(vault.rescan(&fs)).expect("rescan");

    let entry = vault.entries().next().expect("one entry");
    assert_eq!(entry.kind, Kind::Dir);
    assert_ne!(entry.id, old);
}

/// A hard link is an inode the vault knows: it keeps that inode's id, not the path's.
#[test]
fn a_hard_link_replacing_a_note_takes_its_inodes_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/b.md", "b");
    let mut vault = scan(&fs);
    let b = vault.entries().nth(1).expect("b").id;

    fs.remove(Path::new("/vault/a.md"));
    fs.hard_link(Path::new("/vault/b.md"), "/vault/a.md");
    block_on(vault.rescan(&fs)).expect("rescan");

    let after: Vec<_> = vault
        .entries()
        .map(|e| (e.id, e.path.display_name()))
        .collect();
    assert_eq!(after, [(b, "a".to_owned())], "the links collapse to a.md");
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
    assert!(!vault.reidentify(id, saved.identity, saved.stamp.mtime));
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(
        vault.entry(id).map(|e| e.path.display_name()).as_deref(),
        Some("a")
    );
}

/// Vim, VS Code and `git checkout` rename a new file over the note (watcher spec, F5).
#[test]
fn an_external_atomic_save_keeps_the_entry_id() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let id = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    block_on(vault.rescan(&fs)).expect("rescan");

    let entries: Vec<_> = vault.entries().map(|e| (e.id, e.mtime)).collect();
    assert_eq!(entries, [(id, saved.stamp.mtime)]);
    assert_eq!(vault.id_for(saved.identity), Some(id));
}

#[test]
fn reidentify_reclaims_the_id_for_the_inode_our_save_produced() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let ours = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    // The scan read the saved file, moved, before the re-key reached the vault: a fresh id.
    block_on(vault.rescan(&fs)).expect("rescan");
    assert!(vault.entry(ours).is_none());
    assert!(vault.reidentify(ours, saved.identity, saved.stamp.mtime));

    assert_eq!(
        vault.entry(ours).map(|e| e.path.display_name()).as_deref(),
        Some("b")
    );
    assert_eq!(
        vault.entries().count(),
        1,
        "the fresh id is gone, not doubled"
    );
    assert_eq!(vault.id_for(saved.identity), Some(ours));
}

/// A note opened by path inside the root: the scan read its save, but the file stays its entry's.
#[test]
fn reidentify_of_detached_leaves_the_vault_unchanged() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let path = Path::new("/vault/a.md");
    let stamp = block_on(fs.load(path)).expect("load").stamp;
    let saved = block_on(fs.save(path, "after", stamp)).expect("save");
    let mut vault = scan(&fs);
    let minted = vault.entries().next().expect("one entry").id;

    assert!(!vault.reidentify(EntryId::DETACHED, saved.identity, saved.stamp.mtime));

    assert_eq!(vault.entries().map(|e| e.id).collect::<Vec<_>>(), [minted]);
    assert_eq!(vault.id_for(saved.identity), Some(minted));
}

#[test]
fn a_scan_between_a_save_and_its_re_key_keeps_the_note_for_the_next_save() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let ours = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");

    let stamp = block_on(fs.load(path)).expect("load").stamp;
    let first = block_on(fs.save(path, "two", stamp)).expect("save");
    block_on(vault.rescan(&fs)).expect("rescan");
    // The scan gave the saved inode the path's id already: no id moves.
    assert!(!vault.reidentify(ours, first.identity, first.stamp.mtime));

    let second = block_on(fs.save(path, "three", first.stamp)).expect("the next save");
    assert!(!vault.reidentify(ours, second.identity, second.stamp.mtime));
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(vault.absolute(ours).as_deref(), Some(path));
}

/// `RealFs` writes a hard-linked or symlinked note in place: the save keeps the inode a scan read.
#[test]
fn an_in_place_saves_re_key_sets_its_mtime() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");

    fs.insert_file(path, "two");
    let saved = block_on(fs.load(path)).expect("load");
    assert_eq!(
        vault.id_for(saved.identity),
        Some(a),
        "the save kept the inode"
    );
    assert!(!vault.reidentify(a, saved.identity, saved.stamp.mtime));

    assert_eq!(vault.entry(a).expect("kept").mtime, saved.stamp.mtime);
}

/// An `IndexSync` build holds the entries: a re-key that changes no entry must not copy them.
#[test]
fn a_re_key_that_changes_no_entry_leaves_the_entries_shared() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");
    fs.insert_file(path, "two");
    block_on(vault.rescan(&fs)).expect("the scan reads the in-place write");
    let saved = block_on(fs.load(path)).expect("load");
    let held = vault.shared_entries();

    assert!(!vault.reidentify(a, saved.identity, saved.stamp.mtime));

    assert!(Arc::ptr_eq(&held, &vault.shared_entries()));
}

#[test]
fn reidentify_of_a_dead_id_and_an_unknown_identity_changes_nothing() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut vault = scan(&fs);
    let live = vault.entries().next().expect("one entry").id;
    let before: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
    let unknown = FileIdentity {
        device: 1,
        inode: u64::MAX,
    };

    assert!(!vault.reidentify(
        EntryId(u32::MAX),
        unknown,
        Mtime(std::time::SystemTime::UNIX_EPOCH),
    ));

    let after: Vec<_> = vault.entries().map(|e| (e.id, e.path.clone())).collect();
    assert_eq!(before, after);
    assert_eq!(vault.entries().count(), 1);
    assert_eq!(vault.id_for(unknown), None);
    assert!(vault.entry(live).is_some());
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
