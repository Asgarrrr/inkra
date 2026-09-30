use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{TestAppContext, prelude::*};
use inkra_text::Buffer;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::vault::store::AUTOSAVE_DELAY;
use crate::vault::{NoteClosed, NoteStore, TrashFailed};

use crate::test_support::{edit, opened, two_notes};

/// A write pending when the delete is issued would otherwise land after
/// it and recreate the note; `block_save` makes this reproducible.
#[gpui::test]
async fn a_trash_is_not_undone_by_a_write_already_in_flight(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = opened(&store, cx, a).await;
    edit(&buffer, cx, "X");

    let gate = fs.block_save("/vault/a.md");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    let trashed = store.update(cx, |store, cx| store.trash(a, cx));
    // With the write still parked, this is what makes the ordering
    // observable rather than a scheduler coin-flip.
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

/// A write after a delete recreates the file, so a keystroke landing
/// during the delete must not schedule one.
#[gpui::test]
async fn typing_after_a_delete_does_not_bring_the_note_back(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = opened(&store, cx, a).await;

    let gate = fs.block_save("/vault/a.md");
    let trashed = store.update(cx, |store, cx| store.trash(a, cx));
    edit(&buffer, cx, "X");
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
    let (store, a, _b) = two_notes(cx, &fs);

    opened(&store, cx, a).await;

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

/// Closing optimistically and then failing would blank the editor and
/// strand the buffer.
#[gpui::test]
async fn a_failed_trash_keeps_the_note_and_reports_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    opened(&store, cx, a).await;

    let failed = Rc::new(RefCell::new(Vec::new()));
    let _guard = cx.update(|cx| {
        let failed = Rc::clone(&failed);
        cx.subscribe(&store, move |_, event: &TrashFailed, _| {
            failed.borrow_mut().push(*event);
        })
    });

    fs.remove(Path::new("/vault/a.md"));
    assert!(!store.update(cx, |store, cx| store.trash(a, cx)).await);
    cx.run_until_parked();

    assert_eq!(*failed.borrow(), [TrashFailed(a)]);
    store.update(cx, |store, _| {
        assert!(store.buffer_for(a).is_some(), "the buffer is kept");
        assert_eq!(store.current(), Some(a));
    });
}

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

/// Preempting, not cancelling: cancelling would leave the previous save's
/// text in the Trash, the only copy there is.
#[gpui::test]
async fn a_delete_during_the_autosave_window_trashes_the_newest_text(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = opened(&store, cx, a).await;
    edit(&buffer, cx, "X");

    // No `advance_clock`: the write is still waiting out its delay, so
    // only the preempt, not the timer, can have produced it.
    assert!(store.update(cx, |store, cx| store.trash(a, cx)).await);

    assert_eq!(
        fs.trashed(Path::new("/vault/a.md")).as_deref(),
        Some("Xalpha"),
        "the pending write must land before the delete takes the file"
    );
}

/// `Ops::cancel` returning the chain before the line that clears
/// `scheduled` would leave that flag stuck, and the note silently never
/// written again.
#[gpui::test]
async fn a_note_whose_delete_failed_still_autosaves(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, _b) = two_notes(cx, &fs);

    let buffer = opened(&store, cx, a).await;
    edit(&buffer, cx, "X");

    // Not `fs.remove`: the write this delete preempts would recreate the
    // file and the delete would then succeed, proving nothing.
    fs.make_untrashable("/vault/a.md");
    assert!(!store.update(cx, |store, cx| store.trash(a, cx)).await);
    cx.run_until_parked();

    edit(&buffer, cx, "Y");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("YXalpha"),
        "autosave must survive a delete that did not happen"
    );
}

/// A trashing note still binds its buffer (`Trashing(Some)`); an opener must not land on it.
#[gpui::test]
async fn opening_a_note_does_not_land_on_one_already_trashing(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, _a, b) = two_notes(cx, &fs);

    opened(&store, cx, b).await;
    let trashed = store.update(cx, |store, cx| store.trash(b, cx));
    let reopened = store.update(cx, |store, cx| store.open_note(b, cx)).await;

    assert!(reopened.is_none(), "a note being trashed does not open");
    assert!(trashed.await);
}

/// The load lands into `Trashing(Some)` (the merge the gone-note writer needs); no opener gets it.
#[gpui::test]
async fn a_load_landing_during_a_trash_does_not_open(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, _a, b) = two_notes(cx, &fs);
    let gate = fs.block_load("/vault/b.md");
    let opening = store.update(cx, |store, cx| store.open_note(b, cx));
    let trashed = store.update(cx, |store, cx| store.trash(b, cx));

    gate.release();

    assert!(
        opening.await.is_none(),
        "a note being trashed does not open"
    );
    assert!(trashed.await);
}
