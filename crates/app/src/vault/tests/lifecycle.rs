use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Entity, TestAppContext};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;

use crate::test_support::two_notes;

/// Neither load takes the already-open early return, so the second would
/// overwrite the first's entry and the store would forget a live buffer.
#[gpui::test]
async fn two_loads_of_one_note_settle_on_one_buffer(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());

    let (store, a, _b) = two_notes(cx, &fs);

    let first = store.update(cx, |store, cx| store.open_note(a, cx));
    let second = store.update(cx, |store, cx| store.open_note(a, cx));
    let (first, second) = (first.await, second.await);

    let held = store.update(cx, |store, _| store.buffer_for(a).map(Entity::entity_id));
    assert!(held.is_some(), "the note is open");
    assert_eq!(first.map(|buffer| buffer.entity_id()), held);
    assert_eq!(second.map(|buffer| buffer.entity_id()), held);
}

/// The defect: weak handles meant switching notes dropped the last one, so
/// returning reloaded from disk and lost the text and the undo history.
#[gpui::test]
fn switching_away_and_back_preserves_unsaved_text(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());

    let (store, a, b) = two_notes(cx, &fs);

    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    cx.run_until_parked();
    store.update(cx, |store, cx| {
        let buffer = store.buffer_for(a).cloned().expect("a is open");
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset::new(0), "UNSAVED ");
            });
        });
    });

    store.update(cx, |store, cx| store.open_note(b, cx).detach());
    cx.run_until_parked();
    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    cx.run_until_parked();

    store.update(cx, |store, cx| {
        let buffer = store.buffer_for(a).expect("the buffer must still exist");
        assert_eq!(buffer.read(cx).text(), "UNSAVED alpha");
        assert!(
            buffer.read(cx).version() != 0,
            "the undo history survived too"
        );
    });
}

#[gpui::test]
fn switching_notes_writes_the_one_being_left(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (store, a, b) = two_notes(cx, &fs);

    store.update(cx, |store, cx| store.open_note(a, cx).detach());
    store.update(cx, |store, cx| store.open_note(b, cx).detach());
    cx.run_until_parked();
    store.update(cx, |store, cx| store.set_current(a, cx));
    store.update(cx, |store, cx| {
        let buffer = store.buffer_for(a).cloned().expect("a is open");
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| {
                buffer.insert(ByteOffset::new(0), "EDITED ");
            });
        });
    });

    store.update(cx, |store, cx| store.set_current(b, cx));
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("EDITED alpha"),
        "the outgoing note is written, with no clock advance"
    );
}
