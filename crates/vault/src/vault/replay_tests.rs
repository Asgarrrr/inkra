//! A re-key replayed at a scan's swap: the walk's evidence decides (watcher spec, F5).

use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::scan;
use super::*;
use crate::mem_fs::MemFs;

/// Two saves during one held scan: the walk read the first, and the swap replays both re-keys.
#[test]
fn replayed_re_keys_leave_the_entry_at_the_last_saves_mtime() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");

    let stamp = block_on(fs.load(path)).expect("load").stamp;
    let first = block_on(fs.save(path, "two", stamp)).expect("save 1");
    let mid_walk = block_on(Vault::walk(
        &fs,
        Path::new("/vault"),
        &RelPath::root(),
        &Serial,
    ))
    .expect("the scan's walk");
    let second = block_on(fs.save(path, "three", first.stamp)).expect("save 2, during the walk");

    vault = vault.applied(mid_walk);
    assert!(!vault.replay_reidentify(a, first.identity, first.stamp, false));
    assert!(!vault.replay_reidentify(a, second.identity, second.stamp, false));

    assert_eq!(vault.id_for(second.identity), Some(a));
    let disk = block_on(fs.load(path)).expect("load").stamp.mtime;
    assert_eq!(vault.entry(a).expect("kept").mtime, disk);
}

/// A save during a held scan, then a write outside the app to the saved inode, which the walk reads.
#[test]
fn a_replayed_re_key_keeps_the_mtime_the_walk_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");

    let stamp = block_on(fs.load(path)).expect("load").stamp;
    let saved = block_on(fs.save(path, "two", stamp)).expect("save, during the scan");
    fs.insert_file(path, "outside");
    let walk = block_on(Vault::walk(
        &fs,
        Path::new("/vault"),
        &RelPath::root(),
        &Serial,
    ))
    .expect("the scan's walk");

    vault = vault.applied(walk);
    assert!(!vault.replay_reidentify(a, saved.identity, saved.stamp, false));

    let disk = block_on(fs.load(path)).expect("load");
    assert_eq!(
        disk.identity, saved.identity,
        "an in-place write keeps the inode"
    );
    assert_eq!(vault.id_for(saved.identity), Some(a));
    assert_eq!(vault.entry(a).expect("kept").mtime, disk.stamp.mtime);
}

/// The walk read the note before the save: the replay moves the id to the saved inode, at its mtime.
#[test]
fn a_replayed_re_key_after_a_walk_that_read_the_old_inode_takes_the_save() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");
    let loaded = block_on(fs.load(path)).expect("load");
    let walk = block_on(Vault::walk(
        &fs,
        Path::new("/vault"),
        &RelPath::root(),
        &Serial,
    ))
    .expect("the scan's walk");
    let saved = block_on(fs.save(path, "two", loaded.stamp)).expect("save, after the walk read");

    vault = vault.applied(walk);
    assert!(!vault.replay_reidentify(a, saved.identity, saved.stamp, false));

    assert_eq!(vault.id_for(saved.identity), Some(a));
    assert_eq!(vault.id_for(loaded.identity), None, "one id, one identity");
    assert_eq!(vault.entry(a).expect("kept").mtime, saved.stamp.mtime);
}

/// `RealFs` saves a hard-linked note in place: the walk read the inode before the save, so the save wins.
#[test]
fn an_in_place_re_key_replays_the_saves_mtime() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let a = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");
    let walk = block_on(Vault::walk(
        &fs,
        Path::new("/vault"),
        &RelPath::root(),
        &Serial,
    ))
    .expect("the scan's walk");
    fs.insert_file(path, "two");
    let saved = block_on(fs.load(path)).expect("the save, in place");
    let in_place = vault.id_for(saved.identity) == Some(a);
    assert!(in_place, "the live vault maps the kept inode to the note");

    vault = vault.applied(walk);
    assert!(!vault.replay_reidentify(a, saved.identity, saved.stamp, in_place));

    assert_eq!(vault.id_for(saved.identity), Some(a));
    assert_eq!(vault.entry(a).expect("kept").mtime, saved.stamp.mtime);
}
