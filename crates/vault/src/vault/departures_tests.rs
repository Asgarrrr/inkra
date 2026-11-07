//! `Vault::departures`: a new token exactly when a live id leaves.

use std::path::Path;

use futures::executor::block_on;

use super::test_helpers::{id_of, scan};
use super::*;
use crate::mem_fs::MemFs;

const GONE: EntryId = EntryId(900);

#[test]
fn a_scan_that_adds_or_renames_keeps_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut vault = scan(&fs);
    let before = vault.departures().clone();

    fs.insert_file("/vault/b.md", "b");
    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/c.md"))).expect("rename");
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(vault.entries().count(), 2);
    assert_eq!(vault.departures(), &before);
}

#[test]
fn a_scan_that_drops_an_entry_changes_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    fs.insert_file("/vault/b.md", "b");
    let mut vault = scan(&fs);
    let before = vault.departures().clone();

    block_on(fs.trash(Path::new("/vault/a.md"))).expect("trash");
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_ne!(vault.departures(), &before);
}

#[test]
fn a_reidentify_that_drops_the_fresh_id_changes_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let ours = id_of(&vault, "a.md").expect("a");
    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    block_on(vault.rescan(&fs)).expect("rescan");
    let fresh = id_of(&vault, "b.md").expect("b");
    let before = vault.departures().clone();

    assert!(vault.reidentify(ours, saved.identity, saved.stamp));

    assert!(vault.entry(fresh).is_none());
    assert_ne!(vault.departures(), &before);
}

#[test]
fn a_reidentify_in_place_keeps_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let ours = id_of(&vault, "a.md").expect("a");
    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    let before = vault.departures().clone();

    assert!(!vault.reidentify(ours, saved.identity, saved.stamp));

    assert_eq!(vault.departures(), &before);
}

#[test]
fn a_reclaim_changes_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut vault = scan(&fs);
    let other = id_of(&vault, "a.md").expect("a");
    let before = vault.departures().clone();

    assert!(vault.reclaim(GONE, other));

    assert_ne!(vault.departures(), &before);
}

#[test]
fn a_refused_reclaim_keeps_the_token() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut vault = scan(&fs);
    let live = id_of(&vault, "a.md").expect("a");
    let before = vault.departures().clone();

    assert!(!vault.reclaim(live, live));

    assert_eq!(vault.departures(), &before);
}
