//! The scan worker (open-first spec, O2): one scan at a time, requests coalesce, re-keys survive.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::FutureExt;
use gpui::{Entity, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind};

use crate::test_support::{edit, entry_id, index_sync, named, opened, scan_notes, unscanned_notes};
use crate::timeline::Timeline;
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

fn ids(notes: &Entity<NoteStore>, cx: &mut TestAppContext) -> Vec<(String, EntryId)> {
    notes.read_with(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .map(|e| (e.path.as_path().to_string_lossy().into_owned(), e.id))
            .collect()
    })
}

#[gpui::test]
fn a_create_during_a_held_scan_keeps_its_id_and_every_file_keeps_one(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/sub/b.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let before = ids(&notes, cx);

    let gate = fs.block_read_dir("/vault/sub");
    let first = notes.update(cx, NoteStore::rescan);
    cx.run_until_parked();
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "c", cx));
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let c = created
        .now_or_never()
        .expect("landed by the last run_until_parked")
        .ok()
        .flatten()
        .expect("the create lands");
    assert_eq!(first.now_or_never(), Some(Ok(())));
    let after = ids(&notes, cx);
    assert!(after.contains(&("c.md".to_owned(), c)));
    for kept in &before {
        assert!(after.contains(kept), "{kept:?} changed id");
    }
    assert_eq!(after.len(), before.len() + 1);
}

#[gpui::test]
fn a_create_during_the_initial_scan_keeps_its_id(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    let full_pass = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let after_root = ids(&notes, cx);
    assert_eq!(after_root.len(), 2, "{after_root:?}");

    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "c", cx));
    cx.run_until_parked();
    // Holds the create's scan, so the full pass's result shows alone.
    let next = fs.block_read_dir("/vault");
    full_pass.release();
    cx.run_until_parked();
    let after_full = ids(&notes, cx);
    next.release();
    cx.run_until_parked();

    let c = created
        .now_or_never()
        .expect("landed by the last run_until_parked")
        .ok()
        .flatten()
        .expect("the create lands");
    let after = ids(&notes, cx);
    assert!(after.contains(&("c.md".to_owned(), c)));
    for kept in &after_root {
        assert!(
            after_full.contains(kept),
            "{kept:?} changed id in the full pass"
        );
    }
    for kept in &after_full {
        assert!(
            after.contains(kept),
            "{kept:?} changed id in the create's scan"
        );
    }
    assert_eq!(after_full.len(), 3, "{after_full:?}");
    assert_eq!(after.len(), after_full.len() + 1);
}

#[gpui::test]
async fn a_scan_that_swaps_in_between_a_save_and_its_re_key_keeps_the_note(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;

    let gate = fs.block_after_save("/vault/a.md");
    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    drop(notes.update(cx, NoteStore::rescan));
    cx.run_until_parked();
    assert_eq!(
        ids(&notes, cx),
        [("a.md".to_owned(), a)],
        "the saved file took the note's id at its path (watcher spec, F5)"
    );
    gate.release();
    cx.run_until_parked();

    assert_eq!(ids(&notes, cx), [("a.md".to_owned(), a)]);
    assert_eq!(
        named(&links, cx, "a"),
        [a],
        "quick open lists the note's id"
    );
    let disk = fs
        .load(Path::new("/vault/a.md"))
        .await
        .expect("the saved note loads")
        .stamp
        .mtime;
    let mtime = notes.read_with(cx, |notes, cx| {
        let vault = notes.vault().expect("a vault").read(cx);
        vault.entry(a).map(|entry| entry.mtime)
    });
    assert_eq!(mtime, Some(disk));
    edit(&buffer, cx, "2");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("21alpha")
    );
}

#[gpui::test]
fn a_folder_renamed_during_a_held_walk_keeps_its_notes_until_the_next_scan(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/f/n.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let (f, n) = (entry_id(&notes, cx, "f"), entry_id(&notes, cx, "n"));

    let first = fs.block_read_dir("/vault/f");
    drop(notes.update(cx, NoteStore::rescan));
    cx.run_until_parked();
    let renamed = notes.update(cx, |notes, cx| notes.rename(f, "g", cx));
    cx.run_until_parked();
    // The rename's own scan is held at the root, so the first scan's result shows alone.
    let second = fs.block_read_dir("/vault");
    first.release();
    cx.run_until_parked();
    assert_eq!(
        ids(&notes, cx),
        [("f".to_owned(), f), ("f/n.md".to_owned(), n)]
    );

    second.release();
    cx.run_until_parked();
    assert_eq!(renamed.now_or_never(), Some(true));
    assert_eq!(
        ids(&notes, cx),
        [("g".to_owned(), f), ("g/n.md".to_owned(), n)]
    );
}

#[gpui::test]
fn a_scans_swap_counts_in_the_longest_turn(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));

    let scanned = notes.update(cx, NoteStore::rescan);
    cx.run_until_parked();

    assert_eq!(scanned.now_or_never(), Some(Ok(())));
    assert!(cx.read(|cx| cx.global::<Timeline>().longest_turn()) > Duration::ZERO);
}

#[gpui::test]
async fn a_save_during_the_initial_full_pass_keeps_its_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;

    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert!(ids(&notes, cx).contains(&("a.md".to_owned(), a)));
    edit(&buffer, cx, "2");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("21alpha")
    );
}

#[gpui::test]
fn a_failed_root_pass_marks_no_root_step(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.make_unreadable("/vault");
    let _notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();

    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("root -"), "{line}");
    assert!(
        !line.contains("scanned -"),
        "O8: the failed full pass still turns scanned: {line}"
    );
}

#[gpui::test]
fn a_scan_reads_folders_on_several_jobs(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/g/a.md", "");
    fs.insert_file("/vault/h/b.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let (g, h) = (fs.block_read_dir("/vault/g"), fs.block_read_dir("/vault/h"));

    let scanned = notes.update(cx, NoteStore::rescan);
    cx.run_until_parked();
    let reads = (
        fs.read_dirs(Path::new("/vault/g")),
        fs.read_dirs(Path::new("/vault/h")),
    );
    g.release();
    h.release();
    cx.run_until_parked();

    assert_eq!(
        reads,
        (2, 2),
        "one job waits on a folder while another reads the next"
    );
    assert_eq!(scanned.now_or_never(), Some(Ok(())));
}
