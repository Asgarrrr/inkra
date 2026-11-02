//! A frame paints the lines and quote bars the content mask reaches, not the whole document.

use gpui::TestAppContext;

use super::{TALL, dispatching, tall_document};
use crate::editor::layout::painted;

#[gpui::test]
fn a_frame_paints_only_the_lines_in_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    let (lines, _) = painted();
    let fit = window
        .update(&mut cx, |view, window, _| {
            let row = f32::from(view.input.last_layout().unwrap().body_row_height());
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
