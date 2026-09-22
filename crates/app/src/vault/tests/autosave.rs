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

/// Focus is in `EditorView`; the binding is scoped to `AppShell`. Proves
/// the action dispatches up the tree rather than being swallowed.
#[gpui::test]
fn cmd_s_writes_the_current_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    // The caret opens at the end of the buffer (`EditorView::new`).
    visual.simulate_input("X");
    visual.simulate_keystrokes("cmd-s");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX")
    );
}

/// A save changes no text, so no observer may fire: only `grouping_blocked`
/// moved. Swapping `update` back to `as_mut` reintroduces this, because
/// `as_mut` notifies unconditionally (`gpui/src/app.rs:3280-3286`).
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
    // Dirty, so the save reaches the write rather than the early return.
    buffer.update(cx, |buffer, _| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
    });

    // Subscribed after the edit: that notify is legitimate.
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

/// The cadence itself: typing schedules a write, and the clock delivers it
/// with no keystroke from the user.
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

/// The write is scheduled, not immediate. Without this the periodic chain
/// could be replaced by writing on every keystroke and nothing would catch
/// it — a vault on a synced folder would pay for that silently.
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

/// The property an idle debounce does not have (see `AUTOSAVE_DELAY`). Each
/// round types again *before* the previous deadline; the first deadline
/// stands, so the file keeps up with the buffer one interval at a time.
#[gpui::test]
fn continuous_typing_writes_once_per_interval(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut expected = "before".to_owned();
    for round in 1..=3 {
        // The second keystroke lands inside the pending window. Under an
        // idle debounce it would push the deadline past the advance below
        // and this round would write nothing.
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

/// `⌘S` routed through the same queue would be absorbed by the
/// pending-chain guard without preemption, writing nothing until the
/// periodic deadline — breaking the promise the gesture makes. The clock
/// never advances here, so only a preempting `⌘S` produces the write.
#[gpui::test]
fn cmd_s_preempts_a_pending_autosave(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    // Schedules the periodic chain; its deadline is one full delay away.
    visual.simulate_input("X");
    visual.simulate_keystrokes("cmd-s");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("beforeX"),
        "cmd-s must not wait for the deadline it preempted"
    );
}

/// The defect the expectation's placement exists to avoid, in its simplest
/// reachable form. The second write's expectation cannot be the stamp the
/// *load* returned: our own first write already invalidated it. A note
/// taking its expectation from `OpenNote` would mint a conflict file
/// holding its own text on the second keystroke of an ordinary session.
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

/// Same as above, but the second write is *issued* while the first is still
/// in flight: `block_save` holds the periodic write open, so the `⌘S` below
/// is decided against a file our own write hasn't finished changing.
///
/// The two writes cannot overlap at the filesystem: the `⌘S` chain awaits
/// the one it replaced (`schedule_save` takes `previous` rather than
/// dropping it), so the second `write_through` starts after the first
/// returned. Only the quit flush escapes that chaining (see
/// `a_quit_flush_overtakes_the_write_it_overlaps`).
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

/// An external writer changes the note between the load and the write. Its
/// text survives under a new name, and the note holds what is on screen.
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

    // `insert_file` keeps the path's identity, exactly as a second process
    // rewriting a file in place would.
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

/// The defect the cancel channel introduced: `⌘S` installs a chain whose
/// timer is already ready, then a keystroke replaces `note.cancel`, dropping
/// that chain's sender. Treating the resulting `Err(Canceled)` as a cancel
/// would make "write now" write nothing until the next periodic deadline —
/// the clock here never advances past the release, so only a chain that
/// tells a dropped sender from a deliberate one can pass.
#[gpui::test]
fn a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "before");

    let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let gate = fs.block_save("/vault/a.md");

    // Parks the periodic chain inside its write, so everything below queues
    // behind a write that is still in flight.
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

/// `SaveFailed` is the only report a failed write has. Nothing else in the
/// suite can fail one, so without this the emit — and `AppShell`'s
/// subscription to it — can be deleted with the workspace still green.
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

/// The file holds text the buffer never had, so a write of any kind shows —
/// putting the version comparison under test, not just a matching write.
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
