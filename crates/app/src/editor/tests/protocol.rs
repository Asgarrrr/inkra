use gpui::{EntityInputHandler, TestAppContext};

use super::{at, dispatching, with_editor};
use crate::editor::actions::{SelectAll, Undo};

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
        view.select_all(&SelectAll, window, cx);
        view.replace_text_in_range(None, "Z", window, cx);
        assert_eq!(view.buffer.read(cx).text(), "Z");

        view.undo(&Undo, window, cx);
        assert_eq!(view.buffer.read(cx).text(), "abc");
        assert_eq!(view.cursor(), at(3));
    });
}

#[gpui::test]
fn bounds_for_range_follows_the_line_the_range_starts_on(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "aaa\nbbb\nccc");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            // This closure already holds a lease on `view`; a second one panics.
            let element = view.last_bounds.unwrap();
            let first = view.bounds_for_range(0..1, element, window, cx).unwrap();
            let third = view.bounds_for_range(8..9, element, window, cx).unwrap();

            assert_eq!(third.origin.y - first.origin.y, gpui::px(28. * 2.));
            assert_eq!(first.size.height, gpui::px(28.));
        })
        .unwrap();
}

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
