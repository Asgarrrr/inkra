//! The vault follows the disk (watcher spec, W1): each batch of events rescans the folders it names.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, prelude::*};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Pending, RelPath};

use crate::index::IndexSync;
use crate::test_support::{edit, entry_id, named, opened, scan_notes, unscanned_notes, vault};
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

fn indexed(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> (Entity<NoteStore>, Entity<IndexSync>) {
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cx.new(|cx| IndexSync::new(&notes, cx));
    cx.run_until_parked();
    (notes, links)
}

fn paths(notes: &Entity<NoteStore>, cx: &mut TestAppContext) -> Vec<String> {
    notes.read_with(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
            .collect()
    })
}

fn backlinks(links: &Entity<IndexSync>, cx: &mut TestAppContext, note: EntryId) -> Vec<EntryId> {
    links.read_with(cx, |links, _| {
        links
            .index()
            .backlinks(note)
            .into_iter()
            .map(|backlink| backlink.source)
            .collect()
    })
}

/// Listings of each folder so far, in the order given.
fn listings(fs: &MemFs, folders: &[&str]) -> Vec<usize> {
    folders
        .iter()
        .map(|folder| fs.read_dirs(Path::new(folder)))
        .collect()
}

#[gpui::test]
fn a_note_edited_outside_shows_its_new_links_in_backlinks(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let (notes, links) = indexed(cx, &fs);
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));

    fs.insert_file("/vault/f/b.md", "see [[a]]");
    cx.run_until_parked();

    assert_eq!(backlinks(&links, cx, a), [b]);
}

#[gpui::test]
fn notes_created_deleted_or_renamed_outside_follow_in_quick_open(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md"]);
    let (notes, links) = indexed(cx, &fs);
    let a = entry_id(&notes, cx, "a");

    fs.insert_file("/vault/c.md", "");
    futures::executor::block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/d.md")))
        .expect("rename");
    fs.remove(Path::new("/vault/b.md"));
    cx.run_until_parked();

    let c = entry_id(&notes, cx, "c");
    assert_eq!(named(&links, cx, "c"), [c]);
    assert_eq!(named(&links, cx, "d"), [a], "a rename keeps the id");
    assert_eq!(named(&links, cx, "b"), []);
}

#[gpui::test]
fn a_folder_moved_in_brings_its_notes_and_one_removed_takes_its_subtree(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    fs.insert_file("/outside/f/x.md", "");
    fs.insert_file("/outside/f/g/y.md", "");
    let (notes, _links) = indexed(cx, &fs);

    futures::executor::block_on(fs.rename(Path::new("/outside/f"), Path::new("/vault/f")))
        .expect("move in");
    cx.run_until_parked();
    assert_eq!(
        paths(&notes, cx),
        ["f", "f/g", "f/g/y.md", "f/x.md", "a.md"]
    );

    fs.remove(Path::new("/vault/f"));
    cx.run_until_parked();
    assert_eq!(paths(&notes, cx), ["a.md"]);
}

#[gpui::test]
async fn an_autosaves_echo_lists_only_the_notes_folder_and_loads_nothing(cx: &mut TestAppContext) {
    let fs = vault(&["f/a.md", "g/b.md"]);
    let (notes, _links) = indexed(cx, &fs);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;
    let folders = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &folders);
    let loaded = fs.loads(Path::new("/vault/f/a.md"));

    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        listings(&fs, &folders),
        [listed[0], listed[1] + 1, listed[2]],
        "the echo lists the note's folder alone"
    );
    assert_eq!(
        fs.loads(Path::new("/vault/f/a.md")),
        loaded,
        "the index reloads nothing"
    );
}

/// An autosave's echo whose scan starts after the save's re-key, as the batch's 100 ms make it:
/// its swap changes nothing, so the sidebar and the index do no O(n) work for it.
#[gpui::test]
async fn an_echo_read_after_the_saves_re_key_does_not_notify_the_vault(cx: &mut TestAppContext) {
    let fs = vault(&["f/a.md", "g/b.md"]);
    let (notes, _links) = indexed(cx, &fs);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;
    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    let vault = notes.read_with(cx, |notes, _| notes.vault().cloned().expect("a vault"));
    let notified = Rc::new(Cell::new(0));
    let _observed = cx.update(|cx| {
        let notified = Rc::clone(&notified);
        cx.observe(&vault, move |_, _| notified.set(notified.get() + 1))
    });
    let listed = fs.read_dirs(Path::new("/vault/f"));

    let folder = RelPath::new(Path::new("f")).expect("relative");
    let echo = Pending::Folders(BTreeSet::from([folder]));
    notes.update(cx, |notes, cx| notes.rescan_changed(echo, cx));
    cx.run_until_parked();

    assert_eq!(
        fs.read_dirs(Path::new("/vault/f")),
        listed + 1,
        "the echo ran"
    );
    assert_eq!(notified.get(), 0);
}

#[gpui::test]
fn a_rescan_event_runs_one_full_scan(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    let (_notes, _links) = indexed(cx, &fs);
    let folders = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &folders);

    fs.send_rescan("/vault/f");
    cx.run_until_parked();

    let once: Vec<usize> = listed.iter().map(|count| count + 1).collect();
    assert_eq!(listings(&fs, &folders), once);
}

#[gpui::test]
fn batches_during_a_scan_cost_one_more_scan(cx: &mut TestAppContext) {
    let fs = vault(&["f/a.md", "g/b.md"]);
    let (notes, _links) = indexed(cx, &fs);
    let folders = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &folders);

    let gate = fs.block_read_dir("/vault");
    drop(notes.update(cx, NoteStore::rescan));
    cx.run_until_parked();
    for file in ["/vault/f/c.md", "/vault/f/d.md", "/vault/g/e.md"] {
        fs.insert_file(file, "");
        cx.run_until_parked();
    }
    gate.release();
    cx.run_until_parked();

    assert_eq!(
        listings(&fs, &folders),
        [listed[0] + 1, listed[1] + 2, listed[2] + 2],
        "the held scan, then one scoped scan of both folders"
    );
    assert_eq!(
        paths(&notes, cx),
        ["f", "f/a.md", "f/c.md", "f/d.md", "g", "g/b.md", "g/e.md"]
    );
}

#[gpui::test]
fn dotfiles_and_our_temporary_files_start_no_scan(cx: &mut TestAppContext) {
    let fs = vault(&["f/a.md"]);
    let (_notes, _links) = indexed(cx, &fs);
    let folders = ["/vault", "/vault/f"];
    let listed = listings(&fs, &folders);

    fs.insert_file("/vault/.DS_Store", "");
    fs.insert_file("/vault/.git/HEAD", "");
    fs.insert_file("/vault/f/a.md.inkra-1-2.tmp", "");
    cx.run_until_parked();
    assert_eq!(listings(&fs, &folders), listed);

    fs.insert_file("/vault/f/b.md", "");
    cx.run_until_parked();
    assert_eq!(
        listings(&fs, &folders),
        [listed[0], listed[1] + 1],
        "the watch is live"
    );
}

#[gpui::test]
fn a_scoped_read_that_fails_is_picked_up_by_the_next_full_scan(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    let (notes, _links) = indexed(cx, &fs);
    let folders = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &folders);

    fs.make_unreadable("/vault/f");
    let full = fs.block_read_dir("/vault");
    fs.insert_file("/vault/f/d.md", "");
    cx.run_until_parked();
    // `make_unreadable` holds one path: `f` reads again before the full scan does.
    fs.make_unreadable("/nowhere");
    full.release();
    cx.run_until_parked();

    assert_eq!(
        listings(&fs, &folders),
        [listed[0] + 1, listed[1] + 2, listed[2] + 1],
        "the failed scoped read, then one full scan"
    );
    assert_eq!(
        paths(&notes, cx),
        ["f", "f/b.md", "f/d.md", "g", "g/c.md", "a.md"]
    );
}

#[gpui::test]
fn a_scoped_read_that_keeps_failing_costs_one_full_scan(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    let (notes, _links) = indexed(cx, &fs);
    let folders = ["/vault", "/vault/f", "/vault/g"];
    let listed = listings(&fs, &folders);

    fs.make_unreadable("/vault/f");
    fs.insert_file("/vault/f/d.md", "");
    cx.run_until_parked();

    assert_eq!(
        listings(&fs, &folders),
        [listed[0] + 1, listed[1] + 2, listed[2] + 1],
        "the failed scoped read, then one full scan that does not ask again"
    );
    assert_eq!(
        paths(&notes, cx),
        ["f", "f/b.md", "g", "g/c.md", "a.md"],
        "`f` keeps its entries"
    );
}

/// O8 marks a failed initial scan scanned, yet no walk read the vault whole: a batch reads it whole.
#[gpui::test]
fn a_batch_after_a_failed_initial_scan_reads_the_whole_vault(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "g/c.md"]);
    fs.make_unreadable("/vault");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    assert!(paths(&notes, cx).is_empty());

    fs.make_unreadable("/nowhere");
    fs.insert_file("/vault/f/d.md", "");
    cx.run_until_parked();

    assert_eq!(
        paths(&notes, cx),
        ["f", "f/b.md", "f/d.md", "g", "g/c.md", "a.md"]
    );
}
