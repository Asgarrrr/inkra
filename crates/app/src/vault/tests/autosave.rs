use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use gpui::{TestAppContext, prelude::*};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::vault::store::AUTOSAVE_DELAY;
use crate::vault::{ConflictPreserved, NoteStore, SaveFailed};

use super::open_shell;

/// Binding is scoped to `AppShell`, not `EditorView`; proves the action
/// dispatches up the tree.
#[gpui::test]
fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    visual.simulate_keystrokes("cmd-s");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX")
    );
}

/// `as_mut` instead of `update` would notify unconditionally on a save
/// that changes no text.
#[gpui::test]
fn saving_does_not_report_the_buffer_as_changed(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md")))
        .expect("the file was just inserted");
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(loaded.text.clone())));
    let notes = cx.update(|cx| {
        cx.new(|cx| {
            let mut store = NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None, cx);
            store.adopt(
                buffer.clone(),
                PathBuf::from("/vault/a.md"),
                loaded.stamp,
                cx,
            );
            store
        })
    });
    buffer.update(cx, |buffer, _| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
    });

    let notified = Arc::new(AtomicUsize::new(0));
    let _guard = cx.update(|cx| {
        let notified = Arc::clone(&notified);
        cx.observe(&buffer, move |_, _| {
            notified.fetch_add(1, Ordering::SeqCst);
        })
    });

    notes.update(cx, NoteStore::save_current);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("Xbefore"),
        "the save must still have happened"
    );
    assert_eq!(
        notified.load(Ordering::SeqCst),
        0,
        "a save changes no text, so nothing may observe one"
    );
}

#[gpui::test]
fn typing_writes_the_note_without_cmd_s(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX")
    );
}

#[gpui::test]
fn typing_does_not_write_before_the_delay_elapses(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("before"),
        "the note is dirty, but its deadline has not arrived"
    );
}

#[gpui::test]
fn continuous_typing_writes_once_per_interval(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut expected = "before".to_owned();
    for round in 1..=3 {
        // Under an idle debounce, this keystroke would push the deadline
        // past the advance below and the round would write nothing.
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
        visual.simulate_input("Y");
        cx.executor().advance_clock(AUTOSAVE_DELAY / 2);
        cx.run_until_parked();

        expected.push_str("XY");
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some(expected.as_str()),
            "round {round} must have reached disk"
        );
    }
}

#[gpui::test]
fn cmd_s_preempts_a_pending_autosave(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    visual.simulate_keystrokes("cmd-s");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX"),
        "cmd-s must not wait for the deadline it preempted"
    );
}

/// The second write's expectation cannot be the load's stamp: the first
/// write already invalidated it.
#[gpui::test]
fn a_second_write_does_not_conflict_with_the_first(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    visual.simulate_input("Y");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeXY")
    );
    let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    assert_eq!(
        entries.len(),
        1,
        "a write must not conflict with our own previous one"
    );
}

/// The `⌘S` chain awaits the write it replaced, so the two writes cannot
/// overlap at the filesystem even though the second is issued mid-flight.
#[gpui::test]
fn an_overlapping_write_does_not_conflict_with_the_one_in_flight(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let gate = fs.block_save("/vault/a.md");
    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    visual.simulate_input("Y");
    visual.simulate_keystrokes("cmd-s");
    gate.release();
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    assert_eq!(
        entries.len(),
        1,
        "neither write may conflict with the other"
    );
}

#[gpui::test]
fn an_external_change_is_preserved_beside_the_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");
    let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let announced = Arc::new(AtomicUsize::new(0));
    let _guard = cx.update(|cx| {
        let announced = Arc::clone(&announced);
        cx.subscribe(&notes, move |_, _: &ConflictPreserved, _| {
            announced.fetch_add(1, Ordering::SeqCst);
        })
    });

    // Keeps the path's identity, as a second process rewriting in place would.
    fs.insert_file("/vault/a.md", "theirs");

    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX"),
        "the note keeps its path and holds what is on screen"
    );
    let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault"))).expect("read_dir");
    let preserved = entries
        .iter()
        .find(|entry| entry.path.to_string_lossy().contains("conflict"))
        .expect("a conflict file");
    assert_eq!(
        fs.read_to_string(&preserved.path).as_deref(),
        Some("theirs")
    );
    assert_eq!(announced.load(Ordering::SeqCst), 1);
}

/// `⌘S` installs a chain whose timer is already ready, then a keystroke
/// replaces `note.cancel`, dropping that chain's sender. Treating the
/// resulting `Err` as a cancel would make "write now" write nothing.
#[gpui::test]
fn a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let gate = fs.block_save("/vault/a.md");

    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    visual.simulate_keystrokes("cmd-s");
    // Lands while the `⌘S` chain is queued: it replaces the sender that
    // chain is selecting on.
    visual.simulate_input("Y");

    gate.release();
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeXY"),
        "the cmd-s chain must reach its write once the one it queued \
         behind finishes"
    );
}

/// `SaveFailed` is the only report of a failed write; nothing else can
/// fail one, so this is what keeps the emit under test.
#[gpui::test]
fn a_failed_write_reports_it_and_leaves_the_note_dirty(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");
    fs.make_unwritable("/vault/a.md");

    let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let failures = Arc::new(AtomicUsize::new(0));
    let _guard = cx.update(|cx| {
        let failures = Arc::clone(&failures);
        cx.subscribe(&notes, move |_, _: &SaveFailed, _| {
            failures.fetch_add(1, Ordering::SeqCst);
        })
    });

    visual.simulate_input("X");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(failures.load(Ordering::SeqCst), 1, "the write failed once");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("before")
    );
    notes.update(cx, |notes, cx| {
        assert!(
            notes.is_dirty(EntryId::DETACHED, cx),
            "a note whose write failed stays dirty, so the next edit retries"
        );
    });
}

/// The file holds text the buffer never had, so any write at all shows.
#[gpui::test]
fn cmd_s_on_an_untouched_note_leaves_the_file_alone(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "on disk, not in the buffer");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    visual.simulate_keystrokes("cmd-s");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("on disk, not in the buffer")
    );
}
