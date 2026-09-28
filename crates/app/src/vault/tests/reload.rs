//! A clean open note follows the disk; a dirty one keeps its edits (watcher spec, F7 and F8).
//! Rooted at the store, not `AppShell`: the shell's index loads a changed note too, which would
//! hide whether the reload did.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, Subscription, TestAppContext};
use inkra_text::Buffer;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Stamp};

use crate::buffer_events::Reloaded;
use crate::test_support::{conflicts, edit, entry_id, opened, scan_notes};
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

const A: &str = "/vault/a.md";

/// `a` open and current, its text `text`.
async fn open_a(
    cx: &mut TestAppContext,
    text: &str,
) -> (Arc<MemFs>, Entity<NoteStore>, EntryId, Entity<Buffer>) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, text);
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;
    cx.run_until_parked();
    (fs, notes, a, buffer)
}

/// How many `Reloaded` the buffer emits.
fn reloads(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> (Rc<Cell<usize>>, Subscription) {
    let count = Rc::new(Cell::new(0));
    let subscription = cx.update(|cx| {
        let count = Rc::clone(&count);
        cx.subscribe(buffer, move |_, _: &Reloaded, _| count.set(count.get() + 1))
    });
    (count, subscription)
}

fn text(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> String {
    buffer.read_with(cx, |buffer, _| buffer.text().to_owned())
}

fn version(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> u64 {
    buffer.read_with(cx, |buffer, _| buffer.version())
}

fn saved_version(notes: &Entity<NoteStore>, cx: &mut TestAppContext, id: EntryId) -> u64 {
    notes.read_with(cx, |notes, _| notes.open[&id].saved_version())
}

fn expected(notes: &Entity<NoteStore>, cx: &mut TestAppContext, id: EntryId) -> Stamp {
    let writer = notes.read_with(cx, |notes, _| notes.open[&id].writer());
    futures::executor::block_on(writer.expected())
}

fn autosave(cx: &mut TestAppContext) {
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
}

/// The one preserved file's text.
fn preserved_text(fs: &MemFs, preserved: &RefCell<Vec<PathBuf>>) -> Option<String> {
    let preserved = preserved.borrow();
    assert_eq!(preserved.len(), 1, "one conflict file: {preserved:?}");
    fs.read_to_string(&preserved[0])
}

#[gpui::test]
async fn a_dirty_note_changed_on_disk_keeps_its_text_and_its_save_preserves_theirs(
    cx: &mut TestAppContext,
) {
    let (fs, notes, _a, buffer) = open_a(cx, "alpha").await;
    let (preserved, _conflicts) = conflicts(&notes, cx);
    let (reloaded, _reloads) = reloads(&buffer, cx);
    edit(&buffer, cx, "X");

    fs.insert_file(A, "theirs");
    cx.run_until_parked();
    assert_eq!(text(&buffer, cx), "Xalpha");

    autosave(cx);
    assert_eq!(reloaded.get(), 0);
    assert_eq!(text(&buffer, cx), "Xalpha");
    assert_eq!(fs.read_to_string(Path::new(A)).as_deref(), Some("Xalpha"));
    assert_eq!(preserved_text(&fs, &preserved).as_deref(), Some("theirs"));
}

#[gpui::test]
async fn typing_during_the_reloads_load_drops_the_reload(cx: &mut TestAppContext) {
    let (fs, notes, _a, buffer) = open_a(cx, "alpha").await;
    let (preserved, _conflicts) = conflicts(&notes, cx);
    let gate = fs.block_load(A);

    fs.insert_file(A, "theirs");
    cx.run_until_parked();
    assert_eq!(
        fs.loads(Path::new(A)),
        2,
        "the open, then the reload's held load"
    );
    edit(&buffer, cx, "X");
    gate.release();
    cx.run_until_parked();
    assert_eq!(text(&buffer, cx), "Xalpha");

    autosave(cx);
    assert_eq!(text(&buffer, cx), "Xalpha");
    assert_eq!(preserved_text(&fs, &preserved).as_deref(), Some("theirs"));
}

#[gpui::test]
async fn typing_during_the_reloads_diff_refuses_the_apply(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx, "alpha").await;
    let (preserved, _conflicts) = conflicts(&notes, cx);
    let (reloaded, _reloads) = reloads(&buffer, cx);
    let (saved, stamp) = (saved_version(&notes, cx, a), expected(&notes, cx, a));
    let release = notes.update(cx, |notes, _| notes.hold_reload());

    fs.insert_file(A, "theirs");
    cx.run_until_parked();
    assert!(
        notes.read_with(cx, |notes, _| notes.held_reload.is_none()),
        "the reload is parked after its diff"
    );
    edit(&buffer, cx, "X");
    release.send(()).expect("the reload waits on the gate");
    cx.run_until_parked();

    assert_eq!(text(&buffer, cx), "Xalpha");
    assert_eq!(reloaded.get(), 0);
    assert_eq!(saved_version(&notes, cx, a), saved);
    assert_eq!(expected(&notes, cx, a), stamp);

    autosave(cx);
    assert_eq!(preserved_text(&fs, &preserved).as_deref(), Some("theirs"));
}

#[gpui::test]
async fn a_reload_schedules_no_autosave(cx: &mut TestAppContext) {
    let (fs, notes, a, buffer) = open_a(cx, "alpha").await;
    let (reloaded, _reloads) = reloads(&buffer, cx);

    fs.insert_file(A, "theirs");
    cx.run_until_parked();

    assert_eq!(text(&buffer, cx), "theirs");
    assert_eq!(reloaded.get(), 1);
    assert!(!notes.read_with(cx, |notes, _| notes.ops[&a].scheduled));
}

#[gpui::test]
async fn our_own_saves_echo_loads_nothing(cx: &mut TestAppContext) {
    let (fs, _notes, _a, buffer) = open_a(cx, "alpha").await;
    let loads = fs.loads(Path::new(A));

    edit(&buffer, cx, "X");
    let edited = version(&buffer, cx);
    autosave(cx);

    assert_eq!(fs.read_to_string(Path::new(A)).as_deref(), Some("Xalpha"));
    assert_eq!(fs.loads(Path::new(A)), loads);
    assert_eq!(version(&buffer, cx), edited);
}

/// The save's re-key waits for the held scan's swap, so the swap sees the saved mtime as a change.
#[gpui::test]
async fn our_own_saves_echo_during_a_scan_loads_nothing(cx: &mut TestAppContext) {
    let (fs, notes, _a, buffer) = open_a(cx, "alpha").await;
    let loads = fs.loads(Path::new(A));
    let gate = fs.block_read_dir("/vault");
    drop(notes.update(cx, NoteStore::rescan));
    cx.run_until_parked();

    edit(&buffer, cx, "X");
    let edited = version(&buffer, cx);
    autosave(cx);
    assert_eq!(fs.read_to_string(Path::new(A)).as_deref(), Some("Xalpha"));
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new(A)), loads);
    assert_eq!(version(&buffer, cx), edited);
}

/// Their change's check waits behind our save, then reads the pre-save mtime the held scan keeps live.
#[gpui::test]
async fn a_check_queued_before_our_save_loads_once_while_a_scan_holds_the_rekey(
    cx: &mut TestAppContext,
) {
    let (fs, _notes, _a, buffer) = open_a(cx, "alpha").await;
    let (reloaded, _reloads) = reloads(&buffer, cx);
    let loads = fs.loads(Path::new(A));
    edit(&buffer, cx, "X");
    fs.insert_file(A, "theirs");
    cx.run_until_parked();
    assert_eq!(
        fs.loads(Path::new(A)),
        loads,
        "the check waits for the autosave"
    );

    let gate = fs.block_read_dir("/vault");
    autosave(cx);
    assert_eq!(
        fs.loads(Path::new(A)),
        loads + 1,
        "loaded while the echo's scan is held"
    );
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.loads(Path::new(A)), loads + 1);
    assert_eq!(reloaded.get(), 0);
    assert_eq!(text(&buffer, cx), "Xalpha");
}

#[gpui::test]
async fn a_touch_reloads_nothing_and_the_next_save_preserves_nothing(cx: &mut TestAppContext) {
    let (fs, notes, _a, buffer) = open_a(cx, "alpha").await;
    let (preserved, _conflicts) = conflicts(&notes, cx);
    let (reloaded, _reloads) = reloads(&buffer, cx);
    let before = version(&buffer, cx);

    fs.insert_file(A, "alpha");
    cx.run_until_parked();
    assert_eq!(reloaded.get(), 0);
    assert_eq!(version(&buffer, cx), before);

    edit(&buffer, cx, "X");
    autosave(cx);
    assert_eq!(fs.read_to_string(Path::new(A)).as_deref(), Some("Xalpha"));
    assert!(preserved.borrow().is_empty(), "{:?}", preserved.borrow());
}
