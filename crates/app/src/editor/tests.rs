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

/// Scrolling is two builder calls on the view's `div`, and it fails silently
/// if either comes undone — or if the flex parent shrinks `TextElement` back to
/// the window instead of letting it overflow.
///
/// `max_offset` catches all three: it is written only for a tracked handle on a
/// scrolling container, as `content_size - bounds` floored at zero
/// (`gpui/elements/div.rs:2471-2483`). Non-zero therefore means the element
/// really is taller than its viewport and the container really is scrolling it;
/// the companion test below pins the zero case.
#[gpui::test]
fn a_document_taller_than_the_window_has_room_to_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &"line\n".repeat(200));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.scroll_handle.max_offset().y > gpui::px(0.),
                "a 201-line document reported no room to scroll"
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_document_that_fits_has_no_room_to_scroll(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one\ntwo");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.scroll_handle.max_offset().y, gpui::px(0.));
        })
        .unwrap();
}

/// A ragged buffer: line 1 is too short to hold the column lines 0 and 2 can.
/// Offsets — 0..4, 5..7, 8..12.
///
/// One repeated letter on purpose. These assertions are about a *pixel* column
/// resolving back to a byte offset, and the default font is proportional, so
/// two different letters at the same column sit at two different x positions.
const RAGGED: &str = "aaaa\naa\naaaa";

/// The goal column's whole reason to exist: passing over a short line must not
/// permanently give up the column.
#[gpui::test]
fn a_short_line_does_not_swallow_the_goal_column(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, RAGGED);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up right right right");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(3)))
        .unwrap();

    // Line 1 has no column 3, so the caret clamps to its end...
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(7)))
        .unwrap();

    // ...and line 2 does, so it comes back out to it.
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(11)))
        .unwrap();

    cx.simulate_keystrokes("up up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(3)))
        .unwrap();
}

#[gpui::test]
fn a_horizontal_move_between_two_vertical_ones_resets_the_goal(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, RAGGED);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up right right right down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(7)))
        .unwrap();

    // `left` states `Goal::None`, so the next `down` starts from column 1.
    cx.simulate_keystrokes("left down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(9)))
        .unwrap();
}

/// An edit goes through `edit_buffer`, not `move_cursor`. The goal survives
/// that only if something other than the movement gate drops it — which is why
/// it lives in the `Selection` and `EditorState::edit` has to name one.
#[gpui::test]
fn an_edit_between_two_vertical_moves_resets_the_goal(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, RAGGED);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up right right right down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(7)))
        .unwrap();

    // "aaaa\naaXY\naaaa": the caret is now at column 4 of line 1. A stale goal
    // of column 3 would land `down` at offset 13 rather than 14.
    cx.simulate_input("XY");
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "aaaa\naaXY\naaaa");
            assert_eq!(view.cursor(), at(14));
        })
        .unwrap();
}

#[gpui::test]
fn vertical_moves_past_either_end_run_to_the_document_ends(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, RAGGED);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up right right up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    // Running off the top goes through `move_to`, which states `Goal::None`, so
    // the next `down` starts from column 0 — not the column 2 that was in
    // flight when the `up` ran out of lines.
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(5)))
        .unwrap();

    cx.simulate_keystrokes("cmd-down left down");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.cursor(), view.buffer.read(cx).len());
        })
        .unwrap();
}

#[gpui::test]
fn shift_up_and_shift_down_extend_across_lines(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, RAGGED);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up shift-down");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.state.selection().range(), at(0)..at(5));
            assert!(!view.state.selection().reversed());
        })
        .unwrap();

    cx.simulate_keystrokes("shift-up");
    window
        .update(&mut cx, |view, _, _| {
            assert!(view.state.selection().is_empty());
            assert_eq!(view.cursor(), at(0));
        })
        .unwrap();
}

/// One press, one newline.
///
/// `enter` is the only binding whose `Keystroke::key_char` is `Some` on macOS
/// (`gpui_macos/events.rs:369`), so unlike the arrows it is not routed
/// through `doCommandBySelector:`. The platform calls the action first and
/// returns as soon as it reports the key handled (`gpui_macos/window.rs:2728`),
/// which is what stops the input handler also inserting the `\n`.
///
/// This test cannot see that: `dispatch_keystroke` never enters the macOS
/// layer. It proves the keymap and `actions!` wiring, and the platform half is
/// closed by pressing — and holding — Enter in the running app.
#[gpui::test]
fn enter_inserts_exactly_one_newline(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_keystrokes("enter");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "\n");
            assert_eq!(view.buffer.read(cx).line_count(), 2);
            assert_eq!(view.cursor(), at(1));
        })
        .unwrap();
}

#[gpui::test]
fn a_typing_run_broken_by_enter_still_undoes_as_one_step(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("ab");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("cd");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "ab\ncd");
        })
        .unwrap();

    // Enter goes through the same `edit_buffer` gate as typing, so it merges
    // into the surrounding run rather than sealing it.
    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "");
        })
        .unwrap();
}

#[gpui::test]
fn home_and_end_bind_to_the_line_ends_and_cmd_arrows_to_the_document(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\nccc");

    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.cursor(), view.buffer.read(cx).len());
        })
        .unwrap();

    // On the last line: home and end must stop at its ends, not the document's.
    cx.simulate_keystrokes("home");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(8)))
        .unwrap();

    cx.simulate_keystrokes("end");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(11)))
        .unwrap();
}

/// The editor's `p_4()` padding puts the element's origin at (16, 16), and
/// `line_height` is 28, so line `n` occupies y in `[16 + 28n, 16 + 28(n+1))`.
/// Aiming at the middle of a band rather than its edge keeps these from
/// depending on which side of a boundary rounds where.
fn middle_of_line(line: f32) -> gpui::Pixels {
    gpui::px(16. + 28. * line + 14.)
}

#[gpui::test]
fn a_click_lands_on_the_line_it_points_at(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\nccc");
    cx.run_until_parked();

    cx.simulate_click(
        gpui::point(gpui::px(17.), middle_of_line(2.)),
        gpui::Modifiers::none(),
    );
    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.buffer.read(cx);
            assert_eq!(buffer.line_for_offset(view.cursor()), 2);
        })
        .unwrap();
}

/// The mouse listeners sit on the view's `size_full()` div, not on the element,
/// so a click in the empty space under a short document arrives with a y past
/// the last row. Unclamped, that indexes off the end of the layout.
#[gpui::test]
fn a_click_below_the_last_line_clamps_to_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "ab\ncd");
    cx.run_until_parked();

    // Move off the target first. The caret starts at the end of the buffer,
    // which is exactly where this click should land — without this the
    // assertion would hold even if the click never arrived.
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    // Row 5 of a two-line document, and far to the right of any glyph. Kept
    // inside the test window (1536x1095, `gpui/window.rs:81`): a click outside
    // it never hits the div's hitbox, and the handler would not run at all.
    cx.simulate_click(
        gpui::point(gpui::px(400.), middle_of_line(5.)),
        gpui::Modifiers::none(),
    );
    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.buffer.read(cx);
            assert_eq!(buffer.line_for_offset(view.cursor()), 1);
            assert_eq!(view.cursor(), buffer.len());
        })
        .unwrap();
}

/// macOS anchors the input method's candidate window to this rect, so it has to
/// follow the caret down the document rather than staying on line 0.
#[gpui::test]
fn bounds_for_range_follows_the_line_the_range_starts_on(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\nccc");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            // Read the field, don't `read` the entity again: this closure
            // already holds a lease on it, and a second one panics.
            let element = view.last_bounds.unwrap();
            let first = view.bounds_for_range(0..1, element, window, cx).unwrap();
            let third = view.bounds_for_range(8..9, element, window, cx).unwrap();

            assert_eq!(third.origin.y - first.origin.y, gpui::px(28. * 2.));
            // One line tall, not three: a rect spanning the range's whole span
            // would anchor the candidate window nowhere useful.
            assert_eq!(first.size.height, gpui::px(28.));
        })
        .unwrap();
}

/// A composition away from line 0 is where absolute-offset `TextRun`s would
/// stop summing to the line's length and mis-shape it.
#[gpui::test]
fn composition_on_a_later_line_marks_and_commits_there(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\n");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            view.move_cursor(cx, |state, buffer| state.move_to(buffer, at(8), false));
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
            assert_eq!(view.buffer.read(cx).text(), "aaa\nbbb\nか");
            assert_eq!(view.marked_text_range(window, cx), Some(8..9));
        })
        .unwrap();
    // Paints the marked line: the run split is only exercised through prepaint.
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            view.replace_text_in_range(None, "簡", window, cx);
            assert_eq!(view.buffer.read(cx).text(), "aaa\nbbb\n簡");
            assert_eq!(view.marked_text_range(window, cx), None);
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
