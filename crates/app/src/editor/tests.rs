//! Tests that need the gpui edge.
//!
//! Editing semantics are tested in `inkra-editor` without a window. What is
//! left here is what only a window can prove: the UTF-16 protocol conversions,
//! input-method composition, and that real keystrokes reach the buffer.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use gpui::{
    Context, Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window, prelude::*,
};
use inkra_text::{Buffer, ByteOffset};

use super::EditorView;
use super::actions::{Redo, Undo};
use crate::keymap::key_bindings;

/// Drives the entity through a headless window, the only way to reach
/// `EntityInputHandler`, whose methods all require a `Window`.
fn with_editor<R>(
    cx: &mut TestAppContext,
    text: &str,
    f: impl FnOnce(&mut EditorView, &mut Window, &mut Context<EditorView>) -> R,
) -> R {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    window_on(cx, buffer).update(cx, f).unwrap()
}

fn window_on(cx: &mut TestAppContext, buffer: Entity<Buffer>) -> gpui::WindowHandle<EditorView> {
    cx.add_window(move |_, cx| EditorView::new(buffer, cx))
}

/// A focused window with the real key bindings installed, for tests that drive
/// dispatch rather than calling handlers directly.
fn dispatching(
    cx: &mut TestAppContext,
    text: &str,
) -> (gpui::WindowHandle<EditorView>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    let window = window_on(cx, buffer);
    window
        .update(cx, |view, window, cx| window.focus(&view.focus_handle, cx))
        .unwrap();
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

#[gpui::test]
fn the_selection_is_reported_in_utf16_units_not_bytes(cx: &mut TestAppContext) {
    with_editor(cx, "a😀b", |view, window, cx| {
        // 'a'@0, '😀'@1..5 (2 UTF-16 units), 'b'@5.
        view.move_cursor(cx, |state, buffer| state.move_to(buffer, at(5), false));
        let selection = view.selected_text_range(true, window, cx).unwrap();
        assert_eq!(selection.range, 3..3);
    });
}

#[gpui::test]
fn a_utf16_range_spanning_a_surrogate_pair_deletes_the_whole_char(cx: &mut TestAppContext) {
    with_editor(cx, "a😀b", |view, window, cx| {
        view.replace_text_in_range(Some(1..3), "", window, cx);
        assert_eq!(view.buffer.read(cx).text(), "ab");
        assert_eq!(view.cursor(), at(1));
    });
}

#[gpui::test]
fn a_utf16_offset_inside_a_surrogate_pair_snaps_to_a_char_boundary(cx: &mut TestAppContext) {
    with_editor(cx, "a😀b", |view, window, cx| {
        view.replace_text_in_range(Some(2..2), "X", window, cx);
        assert_eq!(view.buffer.read(cx).text(), "a😀Xb");
    });
}

#[gpui::test]
fn text_for_range_round_trips_through_utf16(cx: &mut TestAppContext) {
    with_editor(cx, "héllo", |view, window, cx| {
        let mut actual = None;
        let text = view.text_for_range(1..3, &mut actual, window, cx).unwrap();
        assert_eq!(text, "él");
        assert_eq!(actual, Some(1..3));
    });
}

#[gpui::test]
fn input_method_composition_marks_then_commits(cx: &mut TestAppContext) {
    with_editor(cx, "", |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        assert_eq!(view.buffer.read(cx).text(), "か");
        assert_eq!(view.marked_text_range(window, cx), Some(0..1));

        // The next composition keystroke replaces the marked region.
        view.replace_and_mark_text_in_range(None, "かん", Some(2..2), window, cx);
        assert_eq!(view.buffer.read(cx).text(), "かん");
        assert_eq!(view.marked_text_range(window, cx), Some(0..2));

        view.replace_text_in_range(None, "簡", window, cx);
        assert_eq!(view.buffer.read(cx).text(), "簡");
        assert_eq!(view.marked_text_range(window, cx), None);
    });
}

#[gpui::test]
fn a_whole_composition_undoes_in_one_step(cx: &mut TestAppContext) {
    with_editor(cx, "", |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        view.replace_and_mark_text_in_range(None, "かん", Some(2..2), window, cx);
        view.replace_text_in_range(None, "簡", window, cx);

        // The three composition steps land well inside GROUP_INTERVAL, so they
        // coalesce: one undo discards the whole composition rather than walking
        // back through its intermediate states.
        view.undo(&Undo, window, cx);
        assert_eq!(view.buffer.read(cx).text(), "");
    });
}

#[gpui::test]
fn composition_away_from_the_start_leaves_the_caret_after_the_text(cx: &mut TestAppContext) {
    with_editor(cx, "xy", |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        assert_eq!(view.buffer.read(cx).text(), "xyか");
        assert_eq!(view.cursor(), view.buffer.read(cx).len());
    });
}

#[gpui::test]
fn typing_replaces_the_selection_through_the_protocol(cx: &mut TestAppContext) {
    with_editor(cx, "abc", |view, window, cx| {
        view.select_all(&super::actions::SelectAll, window, cx);
        view.replace_text_in_range(None, "Z", window, cx);
        assert_eq!(view.buffer.read(cx).text(), "Z");

        view.undo(&Undo, window, cx);
        assert_eq!(view.buffer.read(cx).text(), "abc");
        assert_eq!(view.cursor(), at(3));
    });
}

/// End-to-end: real keystrokes through GPUI's dispatch, proving the element
/// registers its input handler on paint and the bindings reach the actions.
#[gpui::test]
fn typing_and_deleting_reach_the_buffer_through_dispatch(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("héllo");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "héllo");
            assert_eq!(view.cursor(), at(6));
        })
        .unwrap();

    cx.simulate_keystrokes("backspace shift-left");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "héll");
            assert_eq!(view.state.selection().range(), at(4)..at(5));
        })
        .unwrap();
}

#[gpui::test]
fn a_typing_run_undoes_in_one_step_through_dispatch(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("hello");
    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "");
        })
        .unwrap();
}

/// Every movement binding, driven as a keystroke.
///
/// The handlers are thin delegations, so without this the bindings, the
/// `actions!` mapping and the repaint gate are only exercised for the two
/// actions the tests above happen to use — a wrong binding would pass
/// `check.sh` unnoticed.
#[gpui::test]
fn every_movement_binding_dispatches(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");

    cx.simulate_keystrokes("home");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    cx.simulate_keystrokes("right right");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(2)))
        .unwrap();

    cx.simulate_keystrokes("left");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(1)))
        .unwrap();

    cx.simulate_keystrokes("end");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(3)))
        .unwrap();

    cx.simulate_keystrokes("shift-left");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.state.selection().range(), at(2)..at(3));
            assert!(view.state.selection().reversed());
        })
        .unwrap();

    cx.simulate_keystrokes("home shift-right");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.state.selection().range(), at(0)..at(1));
            assert!(!view.state.selection().reversed());
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.state.selection().range(),
                at(0)..view.buffer.read(cx).len()
            );
        })
        .unwrap();
}

#[gpui::test]
fn the_delete_and_redo_bindings_dispatch(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");

    cx.simulate_keystrokes("home delete");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "bc");
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "abc");
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-shift-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "bc");
        })
        .unwrap();
}

/// The buffer lives in its own entity, so a view repaints on someone else's
/// edit only through the subscription `EditorView::new` installs. That
/// subscription unsubscribes on `Drop`, so losing it is silent.
///
/// This observes the *second view*, not the buffer: observing the buffer would
/// pass even with the subscription deleted, since the edit notifies the buffer
/// either way. Delete `_buffer_changed` or the `cx.observe` call in
/// `EditorView::new` and this test must fail.
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
            assert_eq!(view.buffer.read(cx).text(), "abcZ");
        })
        .unwrap();
}

#[gpui::test]
fn undoing_with_an_empty_history_leaves_the_text_alone(cx: &mut TestAppContext) {
    with_editor(cx, "abc", |view, window, cx| {
        view.undo(&Undo, window, cx);
        view.redo(&Redo, window, cx);
        assert_eq!(view.buffer.read(cx).text(), "abc");
    });
}
