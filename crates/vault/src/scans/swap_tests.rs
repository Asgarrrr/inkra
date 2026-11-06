//! The swap: re-keys recorded during a scan, replayed onto its result or, if its walk failed,
//! onto the live vault; gone notes reclaimed first (open-first spec, O2; watcher spec, F5 and F9).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use futures::channel::oneshot::Canceled;
use futures::executor::block_on;

use super::test_helpers::{ROOT, run, vault, walked};
use super::*;
use crate::mem_fs::MemFs;
use crate::vault::test_helpers::{id_of, paths, scan};
use crate::{Fs, Saved};

fn path(name: &str) -> PathBuf {
    Path::new(ROOT).join(name)
}

/// An atomic save: the inode moves.
fn save(fs: &MemFs, name: &str) -> Saved {
    let stamp = block_on(fs.load(&path(name))).expect("load").stamp;
    block_on(fs.save(&path(name), "saved", stamp)).expect("save")
}

fn rekey(scans: &mut Scans, live: &mut Vault, id: EntryId, saved: &Saved) -> bool {
    scans.reidentify(live, id, saved.identity, saved.stamp)
}

/// A note deleted on disk, then written back at its path under a new inode, which a scan gave a new id.
fn gone_and_back(fs: &MemFs) -> (Vault, EntryId) {
    let mut live = scan(fs);
    let a = id_of(&live, "a.md").expect("scanned");
    fs.remove(&path("a.md"));
    block_on(live.rescan(fs)).expect("rescan");
    fs.insert_file(path("a.md"), "back");
    block_on(live.rescan(fs)).expect("rescan");
    assert_ne!(id_of(&live, "a.md"), Some(a));
    (live, a)
}

fn claims(open: &[EntryId], claimants: &[(EntryId, Option<&str>)]) -> Claims {
    Claims {
        open: open.iter().copied().collect::<HashSet<_>>(),
        claimants: claimants
            .iter()
            .map(|(id, name)| (*id, name.map(path)))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn a_re_key_during_a_scan_only_records() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();
    let _running = scans.request(&live);

    let before = live.clone();
    let saved = save(&fs, "a.md");

    assert!(!rekey(&mut scans, &mut live, a, &saved));
    assert!(live == before, "recorded, not applied");
}

#[test]
fn a_re_key_with_no_scan_applies_now() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();

    let saved = save(&fs, "a.md");

    assert!(!rekey(&mut scans, &mut live, a, &saved), "no id moved");
    assert_eq!(live.id_for(saved.identity), Some(a));
}

#[test]
fn a_re_key_with_no_scan_says_whether_an_id_moved() {
    let fs = vault(&["a.md"]);
    let (mut live, a) = gone_and_back(&fs);
    let mut scans = Scans::default();

    let loaded = block_on(fs.load(&path("a.md"))).expect("load");

    assert!(scans.reidentify(&mut live, a, loaded.identity, loaded.stamp));
    assert_eq!(id_of(&live, "a.md"), Some(a));
}

/// The walk read the note before the save moved its inode: only the replay knows the new one.
#[test]
fn a_save_re_keyed_during_a_scan_keeps_its_id() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let scanned = walked(running.expect("started"), &fs);
    let saved = save(&fs, "a.md");
    rekey(&mut scans, &mut live, a, &saved);
    let (swapped, _) = scans.swap(&mut live, scanned, &Claims::default());

    assert!(swapped.replaced.is_ok());
    assert_eq!(live.id_for(saved.identity), Some(a));
    let entry = live.entry(a).expect("kept");
    assert_eq!(entry.mtime, saved.stamp.mtime);
}

/// `RealFs` saves a hard-linked note in place: the walk read its inode before the save.
#[test]
fn an_in_place_save_re_keyed_during_a_scan_keeps_the_saves_mtime() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let scanned = walked(running.expect("started"), &fs);
    // `MemFs::save` always moves the inode: the in-place write is `RealFs`'s for a hard link.
    fs.insert_file(path("a.md"), "beta");
    let saved = block_on(fs.load(&path("a.md"))).expect("load");
    scans.reidentify(&mut live, a, saved.identity, saved.stamp);
    scans.swap(&mut live, scanned, &Claims::default());

    let entry = live.entry(a).expect("kept");
    assert_eq!(entry.mtime, saved.stamp.mtime);
}

#[test]
fn two_saves_re_keyed_during_one_scan_keep_the_second_saves_mtime() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let first = save(&fs, "a.md");
    rekey(&mut scans, &mut live, a, &first);
    // The walk sees only the first save; the second lands after it read the folder.
    let scanned = walked(running.expect("started"), &fs);
    let second = save(&fs, "a.md");
    rekey(&mut scans, &mut live, a, &second);
    scans.swap(&mut live, scanned, &Claims::default());

    assert_eq!(live.id_for(second.identity), Some(a));
    assert_eq!(live.entry(a).expect("kept").mtime, second.stamp.mtime);
}

#[test]
fn a_failed_walk_answers_no_request_and_replays_its_re_keys_onto_the_live_vault() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    let mut scans = Scans::default();

    let (mut wait, running) = scans.request(&live);
    let saved = save(&fs, "a.md");
    rekey(&mut scans, &mut live, a, &saved);
    fs.make_unreadable(ROOT);
    let (swapped, next) = run(&mut scans, &mut live, running.expect("started"), &fs);

    assert!(swapped.replaced.is_err());
    assert!(!swapped.notify, "no id moved, and the vault was scanned");
    assert!(next.is_none());
    assert_eq!(wait.try_recv(), Err(Canceled));
    assert_eq!(live.id_for(saved.identity), Some(a));
}

#[test]
fn a_failed_walk_whose_replay_moves_an_id_notifies() {
    let fs = vault(&["a.md"]);
    let (mut live, a) = gone_and_back(&fs);
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let written = block_on(fs.load(&path("a.md"))).expect("load");
    scans.reidentify(&mut live, a, written.identity, written.stamp);
    fs.make_unreadable(ROOT);
    let (swapped, _) = run(&mut scans, &mut live, running.expect("started"), &fs);

    assert!(swapped.replaced.is_err());
    assert!(swapped.notify);
    assert_eq!(id_of(&live, "a.md"), Some(a));
}

/// A note opened by path may lie inside the vault root: still no entry carries `DETACHED`.
#[test]
fn a_detached_note_inside_the_vault_gives_no_entry_its_id() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let scanned = walked(running.expect("started"), &fs);
    let saved = save(&fs, "a.md");
    rekey(&mut scans, &mut live, EntryId::DETACHED, &saved);
    let detached = claims(&[EntryId::DETACHED], &[(EntryId::DETACHED, Some("a.md"))]);
    scans.swap(&mut live, scanned, &detached);

    assert!(live.entries().all(|entry| entry.id != EntryId::DETACHED));
}

#[test]
fn an_echo_that_changes_nothing_does_not_notify() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    let (swapped, _) = run(&mut scans, &mut live, running.expect("started"), &fs);
    assert!(swapped.replaced.is_ok());
    assert!(!swapped.notify);

    fs.insert_file(path("b.md"), "");
    let (_, running) = scans.request(&live);
    let (swapped, _) = run(&mut scans, &mut live, running.expect("started"), &fs);
    assert!(swapped.notify, "a new note");
}

/// A gone note's own save brings its file back during a scan, whose walk mints the file an id.
#[test]
fn a_gone_notes_save_during_a_scan_takes_back_its_id() {
    let fs = vault(&["a.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("scanned");
    fs.remove(&path("a.md"));
    block_on(live.rescan(&fs)).expect("rescan");
    let mut scans = Scans::default();

    let (_, running) = scans.request(&live);
    fs.insert_file(path("a.md"), "written back");
    let saved = block_on(fs.load(&path("a.md"))).expect("load");
    scans.reidentify(&mut live, a, saved.identity, saved.stamp);
    let scanned = walked(running.expect("started"), &fs);
    let gone = claims(&[a], &[(a, Some("a.md"))]);
    let (swapped, _) = scans.swap(&mut live, scanned, &gone);

    assert!(swapped.notify);
    assert_eq!(paths(&live), ["a.md"]);
    assert_eq!(id_of(&live, "a.md"), Some(a));
    assert_eq!(live.id_for(saved.identity), Some(a));
    assert_eq!(live.entry(a).expect("back").mtime, saved.stamp.mtime);
}

#[test]
fn two_claimants_of_one_path_the_lower_id_reclaims_it() {
    let fs = vault(&["x.md", "y.md"]);
    let mut live = scan(&fs);
    let (x, y) = (
        id_of(&live, "x.md").expect("x"),
        id_of(&live, "y.md").expect("y"),
    );
    fs.remove(&path("x.md"));
    fs.remove(&path("y.md"));
    block_on(live.rescan(&fs)).expect("rescan");
    let mut scans = Scans::default();

    fs.insert_file(path("z.md"), "");
    let (_, running) = scans.request(&live);
    let both = claims(&[x, y], &[(y, Some("z.md")), (x, Some("z.md"))]);
    scans.swap(&mut live, walked(running.expect("started"), &fs), &both);

    assert_eq!(id_of(&live, "z.md"), Some(x.min(y)));
    assert!(live.entry(x.max(y)).is_none());
}

#[test]
fn an_open_note_moved_over_a_gone_ones_path_keeps_its_id() {
    let fs = vault(&["a.md", "b.md"]);
    let mut live = scan(&fs);
    let (a, b) = (
        id_of(&live, "a.md").expect("a"),
        id_of(&live, "b.md").expect("b"),
    );
    let mut scans = Scans::default();

    fs.remove(&path("a.md"));
    block_on(fs.rename(&path("b.md"), &path("a.md"))).expect("rename");
    let (_, running) = scans.request(&live);
    let open = claims(&[a, b], &[(a, None), (b, None)]);
    scans.swap(&mut live, walked(running.expect("started"), &fs), &open);

    assert_eq!(id_of(&live, "a.md"), Some(b));
    assert!(live.entry(a).is_none());
}

/// The walk finds nothing new, yet a note opened since claims a row: the swap still notifies.
#[test]
fn a_reclaim_alone_notifies() {
    let fs = vault(&["a.md", "b.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("a");
    let mut scans = Scans::default();
    fs.remove(&path("a.md"));
    block_on(fs.rename(&path("b.md"), &path("a.md"))).expect("rename");
    let (_, running) = scans.request(&live);
    run(&mut scans, &mut live, running.expect("started"), &fs);

    let (_, running) = scans.request(&live);
    let gone = claims(&[a], &[(a, Some("a.md"))]);
    let (swapped, _) = scans.swap(&mut live, walked(running.expect("started"), &fs), &gone);

    assert!(swapped.notify);
    assert_eq!(id_of(&live, "a.md"), Some(a));
}

/// Invariant 9: the new state has no entry for a gone id, so its path comes from the live vault.
#[test]
fn a_claimant_with_no_path_takes_it_from_the_live_vault() {
    let fs = vault(&["a.md", "b.md"]);
    let mut live = scan(&fs);
    let a = id_of(&live, "a.md").expect("a");
    let mut scans = Scans::default();

    fs.remove(&path("a.md"));
    block_on(fs.rename(&path("b.md"), &path("a.md"))).expect("rename");
    let (_, running) = scans.request(&live);
    let gone = claims(&[a], &[(a, None)]);
    let (swapped, _) = scans.swap(&mut live, walked(running.expect("started"), &fs), &gone);

    assert!(swapped.notify);
    assert_eq!(paths(&live), ["a.md"]);
    assert_eq!(id_of(&live, "a.md"), Some(a));
}
