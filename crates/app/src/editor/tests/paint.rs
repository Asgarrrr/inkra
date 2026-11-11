//! A frame paints the lines and quote bars the content mask reaches, not the whole document.

use gpui::{TestAppContext, px};

use super::{TALL, at, dispatching, tall_document};
use crate::editor::layout::painted;
use crate::text_input::painted_selection;

#[gpui::test]
fn a_frame_paints_only_the_lines_in_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    let (lines, _) = painted();
    let fit = window
        .update(&mut cx, |view, window, _| {
            let row = f32::from(view.input.last_layout().unwrap().body_row_height());
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the quotient is non-negative and far below usize::MAX"
            )]
            let fit = (f32::from(window.viewport_size().height) / row).ceil() as usize;
            fit
        })
        .unwrap();
    assert_eq!(lines.start, 0);
    assert!(lines.len() <= fit + 3, "{lines:?} for {fit} rows in view");
    assert!(lines.len() >= fit, "{lines:?} for {fit} rows in view");

    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let (lines, _) = painted();
    assert!(lines.start > 0, "{lines:?}");
    assert_eq!(lines.end, TALL + 1);
}

#[gpui::test]
fn a_frame_paints_the_bars_of_the_lines_in_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &"> quoted\n".repeat(TALL));
    cx.run_until_parked();

    let total = window
        .update(&mut cx, |view, _, _| {
            view.input.last_layout().unwrap().quote_bars().len()
        })
        .unwrap();
    let (_, bars) = painted();
    assert_eq!(bars.start, 0);
    assert!(bars.end < total, "{bars:?} of {total}");

    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    let (lines, bars) = painted();
    assert_eq!(bars.end, total);
    assert!(bars.start > 0, "{bars:?}");
    assert!(bars.len() <= lines.len(), "{bars:?} for {lines:?}");
}

#[gpui::test]
fn a_frame_builds_selection_quads_only_for_the_rows_in_view(cx: &mut TestAppContext) {
    let (_window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-a");
    cx.run_until_parked();

    let quads = painted_selection();
    assert!(quads > 0, "the rows in view are selected");
    assert!(quads < TALL, "{quads} quads for {TALL} lines");
}

#[gpui::test]
fn selection_quads_cover_exactly_the_rows_a_span_reaches(cx: &mut TestAppContext) {
    let text = tall_document();
    let (window, mut cx) = dispatching(cx, &text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            let layout = view.input.last_layout().unwrap();
            let all = at(0)..at(text.len());
            let row = layout.body_row_height();

            // From the middle of row 10 to the middle of row 14.
            let quads = layout.selection_quads(&all, &(row * 10.5..row * 14.5));
            assert_eq!(quads.len(), 5);
            assert_eq!(quads[0].bounds.top(), layout.top_for_row(10));
            assert_eq!(quads[4].bounds.top(), layout.top_for_row(14));

            let above = layout.selection_quads(&(at(0)..at(4)), &(row * 10.5..row * 14.5));
            assert!(
                above.is_empty(),
                "a selection above the span builds nothing"
            );

            let whole = layout.selection_quads(&all, &(px(0.)..layout.height()));
            assert_eq!(whole.len(), layout.row_count());
        })
        .unwrap();
}
