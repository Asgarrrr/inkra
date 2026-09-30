//! No build before the vault's first full scan, and one after it (open-first spec, O5); one at a
//! time under a stream of rescans (coalesced-reconcile spec, R1), and none for an mtime (R2).

use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use gpui::{AppContext as _, TestAppContext};
use inkra_index::live::Status;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind, Mtime, Stamp};

use super::IndexSync;
use super::tests::{backlinks, create, hold_builds, indexed_vault, notes_changed, rescan, status};
use crate::test_support::{entry_id, unscanned_notes};
use crate::timeline::Timeline;
use crate::vault::NoteSaved;

#[gpui::test]
fn the_startup_load_runs_one_build(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let notes = unscanned_notes(cx, fs as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    assert_eq!(status(cx, &links), Status::Scanning);
    let (changed, _subscription) = notes_changed(cx, &links);

    cx.run_until_parked();

    assert_eq!(
        changed.get(),
        1,
        "the root pass and the full pass, one build"
    );
    links.read_with(cx, |links, _| {
        assert_eq!(links.status(), Status::Ready);
        assert_eq!(links.index().names().len(), 2);
    });
}

#[gpui::test]
fn a_save_during_the_initial_scan_marks_no_indexed_step(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let _links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    cx.run_until_parked();

    let a = entry_id(&notes, cx, "a");
    notes.update(cx, |_, cx| {
        cx.emit(NoteSaved {
            id: a,
            text: Arc::from("alpha"),
            stamp: Stamp {
                mtime: Mtime(SystemTime::UNIX_EPOCH),
                size: 5,
            },
        });
    });
    cx.run_until_parked();
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.ends_with("indexed -"), "{line}");

    gate.release();
    cx.run_until_parked();
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("(2 notes"), "{line}");
}

/// Each rescan swaps in while the previous one's build is held, as a watcher's stream would.
#[gpui::test]
fn a_note_changed_during_a_stream_of_rescans_is_indexed_before_it_stops(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    fs.insert_file("/vault/a.md", "[[c]]");
    let mut release = hold_builds(cx, &links);
    create(cx, &notes, "n0");

    for round in 1..4 {
        let next = hold_builds(cx, &links);
        create(cx, &notes, &format!("n{round}"));
        release.send(()).expect("the build waits");
        release = next;
        cx.run_until_parked();
    }

    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);
}

#[gpui::test]
fn a_rescan_moving_only_an_mtime_reloads_that_note_without_a_build(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    cx.run_until_parked();
    let (changed, _subscription) = notes_changed(cx, &links);

    fs.insert_file("/vault/a.md", "[[c]]");
    rescan(cx, &fs, &notes);

    assert_eq!(changed.get(), 0, "no note set built");
    let loads = ["a", "b", "c"].map(|name| fs.loads(Path::new(&format!("/vault/{name}.md"))));
    assert_eq!(loads, [2, 1, 1], "only a reloaded");
    assert_eq!(backlinks(cx, &notes, &links, "c"), ["a"]);
    assert_eq!(backlinks(cx, &notes, &links, "b"), Vec::<String>::new());
}

#[gpui::test]
fn a_create_a_rename_and_a_trash_each_build_the_notes(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    let (changed, _subscription) = notes_changed(cx, &links);

    create(cx, &notes, "c");
    assert_eq!(changed.get(), 1, "create");

    let c = entry_id(&notes, cx, "c");
    let renamed = notes.update(cx, |notes, cx| notes.rename(c, "d", cx));
    cx.run_until_parked();
    drop(renamed);
    assert_eq!(changed.get(), 2, "rename");

    let trashed = notes.update(cx, |notes, cx| notes.trash(c, cx));
    cx.run_until_parked();
    drop(trashed);
    assert_eq!(changed.get(), 3, "trash");
}

#[gpui::test]
fn an_empty_folder_builds_nothing_and_a_folder_renamed_builds_once(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("f/a.md", "")]);
    cx.run_until_parked();
    let (changed, _subscription) = notes_changed(cx, &links);

    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Dir, "empty", cx));
    cx.run_until_parked();
    drop(created);
    assert_eq!(changed.get(), 0, "no note moved");

    let f = entry_id(&notes, cx, "f");
    let renamed = notes.update(cx, |notes, cx| notes.rename(f, "g", cx));
    cx.run_until_parked();
    drop(renamed);
    assert_eq!(changed.get(), 1, "its note's path moved");
}
