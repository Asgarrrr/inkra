use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use gpui::{Entity, TestAppContext, prelude::*};
use inkra_text::Buffer;

use super::{at, window_on};
use crate::editor::EditorView;
use crate::text_input::InputHost;

/// Observes the *second view*, not the buffer: the buffer would notify either
/// way, so this would pass even with the repaint subscription deleted.
#[gpui::test]
fn a_second_view_repaints_when_the_first_one_edits(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("abc")));
    let editing = window_on(cx, buffer.clone());
    let watching = window_on(cx, buffer);

    let watching_view: Entity<EditorView> = watching.update(cx, |_, _, cx| cx.entity()).unwrap();
    let repainted = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = repainted.clone();
    let _subscription = cx.update(|cx| cx.observe(&watching_view, move |_, _| flag.set(true)));

    editing
        .update(cx, |view, window, cx| {
            view.replace_text_in_range(None, "Z", window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert!(
        repainted.get(),
        "the second view was not notified when the first one edited the shared buffer"
    );
    watching
        .update(cx, |view, _, cx| {
            assert_eq!(view.input.buffer().read(cx).text(), "abcZ");
        })
        .unwrap();
}

/// Observes the *view*, not the buffer, which would notify either way — a
/// dropped `Subscription` gives no compiler warning if this silently regresses.
#[gpui::test]
fn editing_after_a_swap_still_notifies(cx: &mut TestAppContext) {
    let first = cx.update(|cx| cx.new(|_| Buffer::new("first")));
    let second = cx.update(|cx| cx.new(|_| Buffer::new("second")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(first, cx)));

    let notified = Arc::new(AtomicUsize::new(0));
    let _guard = cx.update(|cx| {
        let notified = Arc::clone(&notified);
        cx.observe(&editor, move |_, _| {
            notified.fetch_add(1, Ordering::SeqCst);
        })
    });

    editor.update(cx, |view, cx| view.open_buffer(second.clone(), None, cx));
    cx.run_until_parked();
    let before = notified.load(Ordering::SeqCst);

    second.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(at(0), "x"));
        cx.notify();
    });
    cx.run_until_parked();

    assert!(
        notified.load(Ordering::SeqCst) > before,
        "the view must observe the buffer it now holds"
    );
}

#[gpui::test]
fn a_swap_resets_the_caret_and_any_ime_composition(cx: &mut TestAppContext) {
    let first = cx.update(|cx| cx.new(|_| Buffer::new("first")));
    let second = cx.update(|cx| cx.new(|_| Buffer::new("a much longer second note")));
    let editor = cx.update(|cx| cx.new(|cx| EditorView::new(first, cx)));

    editor.update(cx, |view, cx| {
        view.input.state_mut().set_marked_range(Some(at(0)..at(2)));
        view.open_buffer(second, None, cx);
    });

    editor.update(cx, |view, _| {
        assert_eq!(view.cursor(), at(0), "a note opens at the top");
        assert_eq!(
            view.input.state().marked_range(),
            None,
            "an in-flight composition must not survive into another document"
        );
        assert!(
            view.input.last_layout().is_none(),
            "stale geometry must be cleared"
        );
    });
}
