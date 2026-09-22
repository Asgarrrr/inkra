//! Real keystrokes through GPUI's dispatch: bindings, movement, and undo.

use gpui::TestAppContext;

use super::{at, dispatching, with_editor};
use crate::editor::actions::{Redo, Undo};

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

/// A ragged buffer: line 1 is too short to hold the column lines 0 and 2 can.
/// Offsets — 0..4, 5..7, 8..12.
///
/// One repeated letter on purpose. These assertions are about a *pixel*
/// column resolving back to a byte offset, and the default font is
/// proportional, so two different letters at the same column sit at two
/// different x positions.
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

#[gpui::test]
fn undoing_with_an_empty_history_leaves_the_text_alone(cx: &mut TestAppContext) {
    with_editor(cx, "abc", |view, window, cx| {
        view.undo(&Undo, window, cx);
        view.redo(&Redo, window, cx);
        assert_eq!(view.buffer.read(cx).text(), "abc");
    });
}
