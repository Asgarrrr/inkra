use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::{TestAppContext, prelude::*};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::vault::store::AUTOSAVE_DELAY;
use crate::vault::{NoteClosed, NoteStore, TrashFailed};

use super::open_two_notes;

/// The hazard the operation chain exists for. A write pending when the
/// delete is issued would otherwise land *after* it and recreate the note
/// the user just removed — and it is the ordinary case, not an exotic one:
/// deleting a note you have just been typing in is how deleting happens.
///
/// `block_save` is what makes it reproducible. It holds the write open past
/// the turn that issued it, which is the only way the trash can be decided
/// while a write is genuinely in flight.
#[gpui::test]
async fn a_trash_is_not_undone_by_a_write_already_in_flight(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        // The autosave trigger is the observer on the buffer, and only a
        // notify fires it. `EditorView::edit_buffer` leases with `as_mut`,
        // which notifies on drop; reaching the buffer directly, as here,
        // has to say so. Without this there is no write in flight and the
        // test asserts nothing.
        cx.notify();
    });

    // The write starts and parks inside `fs.save`.
    let gate = fs.block_save("/vault/a.md");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    let trashed = store.update(cx, |store, cx| store.trash(a, cx));
    // Everything runnable runs, with the write still parked. The chain is
    // what stops the delete happening here — and this line is what makes
    // that observable: without it both become runnable at once when the
    // gate releases, and which one wins is the scheduler's choice rather
    // than the code's.
    cx.run_until_parked();
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alpha"),
        "the delete must wait for the write it is queued behind"
    );

    gate.release();
    cx.run_until_parked();

    assert!(trashed.await, "the delete must complete");
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "the in-flight write must not put the note back"
    );
    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("Xalpha"),
        "the trash takes the text the write had just landed"
    );
}

/// The note stays open and typable until the delete lands, so a keystroke
/// in that window schedules a write that queues *behind* the trash — and a
/// write running after a delete recreates the file.
///
/// What stops it is `trash` dropping the note from `open` before that write
/// reaches its deadline: `NoteStore::write` starts at `open.get(&id)?` and
/// returns. Stated because it is not obvious, and because the guard that
/// looks like it does this job — refusing to schedule at all once a trash
/// is queued — was tried here and removed: it changed nothing this test
/// could see, and a guard with no observable effect is a guess.
#[gpui::test]
async fn typing_after_a_delete_does_not_bring_the_note_back(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));

    // Gated so the delete is still in flight while the keystroke lands.
    let gate = fs.block_save("/vault/a.md");
    let trashed = store.update(cx, |store, cx| store.trash(a, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        cx.notify();
    });
    gate.release();
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert!(trashed.await);
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")),
        None,
        "an edit landing during the delete must not schedule a write"
    );
}

#[gpui::test]
async fn trashing_the_current_note_closes_it_and_says_so(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));

    let closed = Rc::new(RefCell::new(Vec::new()));
    let _guard = cx.update(|cx| {
        let closed = Rc::clone(&closed);
        cx.subscribe(&store, move |_, event: &NoteClosed, _| {
            closed.borrow_mut().push(*event);
        })
    });

    assert!(store.update(cx, |store, cx| store.trash(a, cx)).await);
    cx.run_until_parked();

    store.update(cx, |store, _| {
        assert!(store.buffer_for(a).is_none(), "the buffer is released");
        assert_eq!(store.current(), None, "nothing is current any more");
    });
    assert_eq!(*closed.borrow(), [NoteClosed(a)]);
}

/// A failed delete leaves the note exactly as it was, and says so. The
/// alternative — closing optimistically and then failing — would blank the
/// editor and strand the buffer, which is worse than a failed write, where
/// the note at least stays visibly dirty.
#[gpui::test]
async fn a_failed_trash_keeps_the_note_and_reports_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));

    let failed = Rc::new(RefCell::new(Vec::new()));
    let _guard = cx.update(|cx| {
        let failed = Rc::clone(&failed);
        cx.subscribe(&store, move |_, event: &TrashFailed, _| {
            failed.borrow_mut().push(*event);
        })
    });

    // Removed behind the store's back, so the delete finds nothing.
    fs.remove(Path::new("/vault/a.md"));
    assert!(!store.update(cx, |store, cx| store.trash(a, cx)).await);
    cx.run_until_parked();

    assert_eq!(*failed.borrow(), [TrashFailed(a)]);
    store.update(cx, |store, _| {
        assert!(store.buffer_for(a).is_some(), "the buffer is kept");
        assert_eq!(store.current(), Some(a));
    });
}

/// `DETACHED` names the file given on the command line. It has no vault
/// entry, so there is nothing to remove and no row to remove it from — and
/// `current` *can* hold it, which is how a future caller might reach here.
#[gpui::test]
async fn trashing_the_command_line_note_does_nothing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md"))).expect("load");
    let store = cx.update(|cx| {
        cx.new(|cx| {
            let mut store = NoteStore::new(Arc::clone(&fs) as Arc<dyn Fs>, None, cx);
            let buffer = cx.new(|_| Buffer::new(loaded.text.clone()));
            store.adopt(buffer, PathBuf::from("/vault/a.md"), loaded.stamp, cx);
            store
        })
    });

    assert!(
        !store
            .update(cx, |store, cx| store.trash(EntryId::DETACHED, cx))
            .await
    );
    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alpha")
    );
}

/// Preempting rather than cancelling, stated as an outcome: the delete is
/// issued while the write is still counting down, and the copy that reaches
/// the Trash is the one the user last typed.
///
/// Cancelling the pending write instead leaves the previous save's text in
/// the Trash — which is the only copy there is, so the edits are gone for
/// good. This test is the difference between the two.
#[gpui::test]
async fn a_delete_during_the_autosave_window_trashes_the_newest_text(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        cx.notify();
    });

    // No `advance_clock`: the write is still waiting out its delay.
    assert!(store.update(cx, |store, cx| store.trash(a, cx)).await);

    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("Xalpha"),
        "the pending write must land before the delete takes the file"
    );
}

/// A failed delete has to leave the note working, not merely present.
///
/// Sending on `Ops::cancel` makes the write chain return at its select,
/// before the line that clears `scheduled` — so the flag stays set, every
/// later edit is absorbed by it, and the note is never written again. The
/// note looks fine and silently stops saving.
#[gpui::test]
async fn a_note_whose_delete_failed_still_autosaves(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (store, a, _b) = open_two_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let buffer = store
        .update(cx, |store, cx| store.open_note(a, cx))
        .await
        .expect("a opens");
    store.update(cx, |store, cx| store.set_current(a, cx));
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        cx.notify();
    });

    // Refused by the platform, so the delete fails and the note stays. Not
    // `fs.remove`: the write this delete preempts would recreate the file
    // and the delete would then succeed.
    fs.make_untrashable("/vault/a.md");
    assert!(!store.update(cx, |store, cx| store.trash(a, cx)).await);
    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "Y"));
        cx.notify();
    });
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("YXalpha"),
        "autosave must survive a delete that did not happen"
    );
}
