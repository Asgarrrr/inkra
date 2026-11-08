//! Headless glyphs all advance by the same amount, so a row holds a fixed
//! number of chars and every break below is computable; fixtures keep clear of a break landing exactly on the column's edge.

use gpui::{ClipboardItem, Pixels, TestAppContext, px, size};

use super::geometry::{HEADING_MARGIN, LINE_HEIGHT, MARGIN, PADDING, VIEWPORT};
use super::{at, dispatching};
use crate::editor::EditorView;
use crate::text_input::InputHost;

const WINDOW_WIDTH: f32 = 1920.;

/// Seventeen four-char words: 68 chars, fits a 70 `ch` row with room to spare.
fn one_row() -> String {
    "aaa ".repeat(17)
}

/// Twenty-five four-char words: 100 chars, breaks once at char 68.
fn two_rows() -> String {
    "aaa ".repeat(25)
}

fn column_width(view: &EditorView) -> Pixels {
    view.input.last_bounds().unwrap().size.width
}

#[gpui::test]
fn a_line_longer_than_the_column_takes_two_rows(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &two_rows());
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_count(), 2);
            assert_eq!(layout.top_for_row(1), px(LINE_HEIGHT));
            assert_eq!(
                view.input.last_bounds().unwrap().size.height,
                px(2. * LINE_HEIGHT)
            );
        })
        .unwrap();
}

/// Painted rows fill exactly the height taffy reserved, proving `prepaint` used the layout taffy sized.
#[gpui::test]
fn the_column_is_seventy_ch_and_centred(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &two_rows());
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, _| {
            let style = view.input.last_layout().unwrap().style().clone();
            let column = crate::editor::element::column_width(&style, window);
            let bounds = view.input.last_bounds().unwrap();
            assert_eq!(bounds.size.width, column);
            let pane = px(WINDOW_WIDTH - 2. * PADDING);
            assert_eq!(bounds.origin.x, px(PADDING) + (pane - column) / 2.);

            let layout = view.input.last_layout().unwrap();
            #[allow(clippy::cast_precision_loss)]
            let rows = layout.row_count() as f32;
            assert_eq!(bounds.size.height, px(LINE_HEIGHT) * rows);
        })
        .unwrap();
}

#[gpui::test]
fn a_narrow_window_wraps_at_the_pane(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &one_row());
    cx.simulate_resize(size(px(400.), px(600.)));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(
                column_width(view),
                px(400. - 2. * PADDING - 2. * HEADING_MARGIN)
            );
            let layout = view.input.last_layout().unwrap();
            assert!(layout.row_count() > 1, "68 chars cannot fit 216 px");
            #[allow(clippy::cast_precision_loss)]
            let rows = layout.row_count() as f32;
            assert_eq!(
                view.input.last_bounds().unwrap().size.height,
                px(LINE_HEIGHT) * rows
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_caret_at_a_wrap_paints_at_the_start_of_the_next_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &two_rows());
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_for_offset(at(68)), 1);
            assert_eq!(
                layout.position_for_offset(at(68)),
                gpui::point(px(0.), px(LINE_HEIGHT))
            );
            assert_eq!(layout.row_for_offset(at(67)), 0);
        })
        .unwrap();
}

#[gpui::test]
fn an_offset_past_the_end_lands_on_the_last_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one\ntwo\nthree");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_for_offset(at(3)), 0);
            assert_eq!(layout.row_for_offset(at(4)), 1);
            assert_eq!(layout.row_for_offset(at(13)), 2);
            assert_eq!(layout.row_for_offset(at(40)), 2);
        })
        .unwrap();
}

#[gpui::test]
fn up_and_down_move_by_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &format!("{}\nshort", two_rows()));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");

    let cursor =
        |cx: &mut gpui::VisualTestContext| window.update(cx, |view, _, _| view.cursor()).unwrap();

    cx.simulate_keystrokes("down");
    assert_eq!(cursor(&mut cx), at(68), "row 1 of the same line");
    cx.simulate_keystrokes("down");
    assert_eq!(cursor(&mut cx), at(101), "the next logical line");
    cx.simulate_keystrokes("up");
    assert_eq!(cursor(&mut cx), at(68));
    cx.simulate_keystrokes("up");
    assert_eq!(cursor(&mut cx), at(0));
}

#[gpui::test]
fn the_goal_column_survives_a_short_row(cx: &mut TestAppContext) {
    // Row 0: chars 0..68; row 1: "aaa " x 8 = 32 chars, 68..100; then a one-row line of 68.
    let text = format!("{}\n{}", two_rows(), one_row());
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    for _ in 0..50 {
        cx.simulate_keystrokes("right");
    }

    cx.simulate_keystrokes("down");
    let clamped = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(
        clamped,
        at(100),
        "row 1 is 32 chars: the caret clamps to its end"
    );

    cx.simulate_keystrokes("down");
    let back = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(back, at(101 + 50), "column 50 again, on the next line");
}

/// Asserted inside the same update as the edit, before any draw, so the only
/// layout the reveal could have used is the one the gate shaped.
#[gpui::test]
fn an_edit_that_wraps_reveals_the_new_row_before_the_next_paint(cx: &mut TestAppContext) {
    // 40 one-row lines, then a 69-char line: one more word start at 68 wraps it.
    let text = format!("{}{}b", "x\n".repeat(40), one_row());
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            assert_eq!(view.input.last_layout().unwrap().row_count(), 41);
            view.replace_text_in_range(None, "bb", window, cx);

            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_count(), 42, "the edit wrapped the last line");
            assert_eq!(layout.row_for_offset(view.cursor()), 41);
            let row_bottom = PADDING + LINE_HEIGHT * 42.;
            let expected = VIEWPORT - MARGIN - row_bottom;
            assert_eq!(view.viewport.handle().offset().y, px(expected));
        })
        .unwrap();
}

/// A pasted URL has no spaces, so gpui breaks it mid-word. Pasted at the end,
/// gpui clamps the offset to the content and the margin cannot be honoured.
#[gpui::test]
fn pasting_a_long_url_reveals_its_last_row(cx: &mut TestAppContext) {
    let text = format!("{}\n{}", "x\n".repeat(60), "y\n".repeat(40));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    for _ in 0..60 {
        cx.simulate_keystrokes("down");
    }
    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "https://example.com/{}",
        "a".repeat(300)
    )));
    cx.simulate_keystrokes("cmd-v");

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let row = layout.row_for_offset(view.cursor());
            assert!(row > 61, "the URL wrapped onto rows past line 60");
            #[allow(clippy::cast_precision_loss)]
            let row_bottom = PADDING + LINE_HEIGHT * (row as f32 + 1.);
            let expected = VIEWPORT - MARGIN - row_bottom;
            assert_eq!(view.viewport.handle().offset().y, px(expected));
        })
        .unwrap();
}

#[gpui::test]
fn a_row_never_starts_inside_a_grapheme(cx: &mut TestAppContext) {
    // Two chars of prefix put every cluster start on an even char index, so
    // chars 69 and 70 — either side of the break — are both marks.
    let text = format!("xx{}", "e\u{301}\u{302}".repeat(40));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_keystrokes("down");

    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.input.buffer().read(cx);
            let layout = view.input.last_layout().unwrap();
            assert!(layout.row_count() > 1);
            for row in 0..layout.row_count() {
                let start = layout.offset_on_row(row, px(0.));
                assert!(
                    on_boundary(buffer, start),
                    "row {row} starts inside a cluster"
                );
            }
            assert!(on_boundary(buffer, view.cursor()));
        })
        .unwrap();
}

fn on_boundary(buffer: &inkra_text::Buffer, offset: inkra_text::ByteOffset) -> bool {
    offset == at(0) || buffer.next_grapheme(buffer.previous_grapheme(offset)) == offset
}

#[gpui::test]
fn every_offset_round_trips_through_its_position(cx: &mut TestAppContext) {
    let text = format!("{}\nhttps://example.com/{}", two_rows(), "a".repeat(150));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, cx| {
            let buffer = view.input.buffer().read(cx);
            let layout = view.input.last_layout().unwrap();
            assert!(layout.row_count() >= 5, "both lines wrapped");
            let mut offset = at(0);
            loop {
                let back = layout.offset_for_position(layout.position_for_offset(offset));
                assert_eq!(back, offset);
                if offset == buffer.len() {
                    break;
                }
                offset = buffer.next_grapheme(offset);
            }
        })
        .unwrap();
}

#[gpui::test]
fn a_click_past_a_wrapped_row_stays_on_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &two_rows());
    cx.run_until_parked();

    cx.simulate_click(
        gpui::point(px(1800.), px(PADDING + LINE_HEIGHT / 2.)),
        gpui::Modifiers::none(),
    );
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.cursor(), at(67));
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_for_offset(view.cursor()), 0);
        })
        .unwrap();
}

#[gpui::test]
fn a_selection_paints_one_quad_per_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &format!("{}\nbb", two_rows()));
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let quads = layout.selection_quads(&(at(0)..at(103)));
            assert_eq!(quads.len(), 3);

            let width = |quad: &gpui::PaintQuad| quad.bounds.size.width;
            // Row 0's wrap x is its `caret_end` (67) plus the trailing space;
            // offset 68 directly would answer for row 1 instead.
            let one_char = layout.x_for_offset(at(1));
            let row_0 = layout.x_for_offset(at(67)) + one_char;
            assert_eq!(width(&quads[0]), row_0, "row 0 runs to the wrap, no stub");
            let row_1 = layout.x_for_offset(at(100));
            let stub = px(LINE_HEIGHT * 0.25);
            assert_eq!(width(&quads[1]), row_1 + stub, "row 1 ends the line");
            assert_eq!(quads[1].bounds.origin.y, px(LINE_HEIGHT));
        })
        .unwrap();
}

#[gpui::test]
fn ime_bounds_follow_the_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &two_rows());
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            let element = view.input.last_bounds().unwrap();
            let bounds = view.bounds_for_range(70..72, element, window, cx).unwrap();
            assert_eq!(bounds.origin.y, element.origin.y + px(LINE_HEIGHT));
            let two_chars = view.input.last_layout().unwrap().x_for_offset(at(2));
            assert_eq!(bounds.origin.x, element.origin.x + two_chars);
        })
        .unwrap();
}

fn caret_after(cx: &mut TestAppContext, keys: &str) -> inkra_text::ByteOffset {
    let (window, mut cx) = dispatching(cx, &format!("{}\nshort", two_rows()));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_keystrokes(keys);
    window.update(&mut cx, |view, _, _| view.cursor()).unwrap()
}

/// Ten chars into row 0: End stops one grapheme short of the wrap, before the trailing space.
#[gpui::test]
fn end_stops_at_the_visual_row_and_stays(cx: &mut TestAppContext) {
    let keys = "right right right right right right right right right right end";
    assert_eq!(caret_after(cx, keys), at(67));
    assert_eq!(caret_after(cx, &format!("{keys} end")), at(67));
}

/// Ten chars into row 1: Home stops at the row's start, not the line's.
#[gpui::test]
fn home_stops_at_the_visual_row_and_stays(cx: &mut TestAppContext) {
    let keys = "down right right right right right right right right right right home";
    assert_eq!(caret_after(cx, keys), at(68));
    assert_eq!(caret_after(cx, &format!("{keys} home")), at(68));
}

#[gpui::test]
fn cmd_right_is_end(cx: &mut TestAppContext) {
    assert_eq!(caret_after(cx, "right right cmd-right"), at(67));
}

#[gpui::test]
fn cmd_left_is_home(cx: &mut TestAppContext) {
    assert_eq!(caret_after(cx, "down right right cmd-left"), at(68));
}

#[gpui::test]
fn ctrl_a_goes_to_the_start_of_the_logical_line(cx: &mut TestAppContext) {
    assert_eq!(caret_after(cx, "down right right ctrl-a"), at(0));
}

#[gpui::test]
fn ctrl_e_goes_to_the_end_of_the_logical_line(cx: &mut TestAppContext) {
    assert_eq!(caret_after(cx, "right right ctrl-e"), at(100));
}

/// Both breaks in a cluster wider than the pane snap to its end, leaving a row
/// with no caret position of its own; up/down must step over it, not get stuck.
#[gpui::test]
fn up_steps_over_a_row_with_no_caret_position(cx: &mut TestAppContext) {
    // Five glyphs in one cluster, in a text column two glyphs wide.
    let text = format!("ab{}cd", "e\u{301}\u{302}\u{303}\u{304}");
    let (window, mut cx) = dispatching(cx, &text);
    cx.simulate_resize(size(px(2. * PADDING + 2. * HEADING_MARGIN + 25.), px(600.)));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");

    let rows = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().row_count()
        })
        .unwrap();
    for _ in 0..rows {
        cx.simulate_keystrokes("up");
    }
    let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
    assert_eq!(caret, at(0), "up stuck below a collapsed row");
}
