//! Row geometry around headings. Headless glyphs advance `0.6 × font_size`, so an H1 row holds 37 chars of the 756 px column.

use gpui::{TestAppContext, point, px};

use super::geometry::{H1_LINE_HEIGHT as H1, LINE_HEIGHT, MARGIN, TEXT_TOP, VIEWPORT};
use super::{at, dispatching};
use crate::text_input::InputHost;

const H2: f32 = 36.;
const H3: f32 = 30.;

#[gpui::test]
fn an_atx_heading_row_is_taller_than_body(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_height(0), px(H1));
            assert_eq!(layout.row_height(1), px(LINE_HEIGHT));
            assert_eq!(layout.top_for_row(1), px(H1));
            assert_eq!(layout.height(), px(H1 + LINE_HEIGHT));
            assert_eq!(
                view.input.last_bounds().unwrap().size.height,
                px(H1 + LINE_HEIGHT)
            );
        })
        .unwrap();
}

#[gpui::test]
fn each_level_takes_its_own_height(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "x\n# a\n## b\n### c\n##### e");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let heights: Vec<_> = (0..5).map(|row| layout.row_height(row)).collect();
            assert_eq!(heights, [LINE_HEIGHT, H1, H2, H3, LINE_HEIGHT].map(px));
        })
        .unwrap();
}

#[gpui::test]
fn a_setext_underline_keeps_body_height(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "Title\n===\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let tops: Vec<_> = (0..3).map(|row| layout.top_for_row(row)).collect();
            assert_eq!(tops, [0., H1, H1 + LINE_HEIGHT].map(px));
        })
        .unwrap();
}

/// The caret starts at the buffer's end, so `# ` is hidden: 60 chars at 20.4 px wrap after nine words (734.4 px of 756).
#[gpui::test]
fn a_wrapped_heading_steps_its_rows_by_its_own_height(cx: &mut TestAppContext) {
    let text = format!("# {}\nbody", "aaa ".repeat(15));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_count(), 3);
            let tops: Vec<_> = (0..=3).map(|row| layout.top_for_row(row)).collect();
            assert_eq!(tops, [0., H1, 2. * H1, 2. * H1 + LINE_HEIGHT].map(px));
        })
        .unwrap();
}

/// D12: `# ` hangs in the margin once revealed, so the text starts at zero in both states.
#[gpui::test]
fn a_heading_starts_at_zero_at_its_own_size_in_both_states(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nT");
    cx.run_until_parked();
    let xs = |cx: &mut gpui::VisualTestContext| {
        window
            .update(cx, |view, _, _| {
                let layout = view.input.last_layout().unwrap();
                [2, 3, 5].map(|offset| f32::from(layout.x_for_offset(at(offset))))
            })
            .unwrap()
    };
    let concealed = xs(&mut cx);
    // A view opens with the caret at the end; move it onto the heading so `# ` shows.
    cx.simulate_keystrokes("cmd-up");
    let revealed = xs(&mut cx);
    for [start, end, body] in [concealed, revealed] {
        assert!(start.abs() < 0.01, "{start}");
        assert!((end - 0.6 * 34.).abs() < 0.01, "{end}");
        assert!((body - 0.6 * 18.).abs() < 0.01, "{body}");
    }
}

#[gpui::test]
fn a_click_below_a_heading_lands_on_the_row_under_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert!(layout.offset_for_position(point(px(0.), px(H1 - 1.))) <= at(3));
            assert_eq!(
                layout.offset_for_position(point(px(0.), px(H1 + 1.))),
                at(4)
            );
        })
        .unwrap();
}

#[gpui::test]
fn the_caret_is_as_tall_as_its_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.caret_bounds(at(1)).size.height, px(H1));
            let body = layout.caret_bounds(at(5));
            assert_eq!(body.size.height, px(LINE_HEIGHT));
            assert_eq!(body.origin.y, px(H1));
        })
        .unwrap();
}

#[gpui::test]
fn the_ime_rectangle_on_a_heading_is_as_tall_as_its_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, window, cx| {
            let origin = view.input.last_bounds().unwrap();
            let bounds = view.bounds_for_range(2..4, origin, window, cx).unwrap();
            assert_eq!(bounds.size.height, px(H1));
        })
        .unwrap();
}

#[gpui::test]
fn a_selection_quad_is_as_tall_as_its_own_row(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# T\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let quads = layout.selection_quads(&(at(0)..at(8)));
            let heights: Vec<_> = quads.iter().map(|quad| quad.bounds.size.height).collect();
            assert_eq!(heights, [px(H1), px(LINE_HEIGHT)]);
            assert_eq!(quads[1].bounds.origin.y, px(H1));
        })
        .unwrap();
}

/// Goal x is in pixels, so it survives rows of different glyph widths: body x 21.6 is
/// index 1 of the concealed `Title` at 20.4 px per char, then index 2 of `body` again.
#[gpui::test]
fn up_and_down_through_a_heading_keep_the_goal_x(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "# Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("left left");
    cx.simulate_keystrokes("up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(3)))
        .unwrap();
    cx.simulate_keystrokes("down");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(10)))
        .unwrap();
}

#[gpui::test]
fn typing_a_hash_grows_the_line_and_undo_shrinks_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "Title\nbody");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input("# ");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_height(0), px(H1));
            assert_eq!(layout.top_for_row(1), px(H1));
        })
        .unwrap();
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            assert_eq!(layout.row_height(0), px(LINE_HEIGHT));
        })
        .unwrap();
}

/// The margin counts body lines, as Zed's `vertical_scroll_margin` does, even with the caret on an H1:
/// the heading's bottom is at `TEXT_TOP + 60 × 28 + 44 = 1740`, so the offset is `1080 − 84 − 1740 = −744`.
#[gpui::test]
fn the_reveal_margin_stays_in_body_lines_on_a_heading(cx: &mut TestAppContext) {
    // Lines below the heading keep gpui's end-of-document clamp from masking the margin.
    let text = format!("{}# End\n{}", "x\n".repeat(60), "x\n".repeat(20));
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_keystrokes(&["down"; 60].join(" "));
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            // Goal x 0 lands after `# `: the body never maps into the marker (D12).
            assert_eq!(view.cursor(), at(122));
            let expected = VIEWPORT - MARGIN - (TEXT_TOP + 60. * LINE_HEIGHT + H1);
            assert_eq!(view.scroll_handle.offset().y, px(expected));
        })
        .unwrap();
}
