use gpui::TestAppContext;

use super::{at, dispatching, geometry};
use crate::editor::actions::Down;

/// `max_offset` is non-zero only for a tracked handle on a genuinely
/// overflowing container, so it catches wiring, flex-shrink, and layout bugs alike.
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

const TALL: usize = 200;

fn tall_document() -> String {
    "line\n".repeat(TALL)
}

#[gpui::test]
fn reaching_the_end_of_a_tall_document_scrolls_it_into_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, cx| {
            let offset = view.scroll_handle.offset().y;
            assert!(offset < gpui::px(0.), "a 201-line document did not scroll");

            #[allow(clippy::cast_precision_loss)]
            let last = view.buffer.read(cx).line_count() as f32 - 1.;
            assert!(
                geometry::line_is_visible(last, offset),
                "the last line is at {:?}, outside the viewport",
                geometry::top_of_line(last, offset)
            );
        })
        .unwrap();
}

#[gpui::test]
fn returning_to_the_start_scrolls_back_to_the_top(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, _| {
            assert!(view.scroll_handle.offset().y < gpui::px(0.));
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.scroll_handle.offset().y, gpui::px(0.));
        })
        .unwrap();
}

/// The exact offset is asserted, not just visibility: a reveal without a
/// margin still satisfies `line_is_visible`, hiding the bug this catches.
#[gpui::test]
fn revealing_a_line_from_below_leaves_a_margin_under_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).line_for_offset(view.cursor()), 40);
            assert_eq!(
                view.scroll_handle.offset().y,
                geometry::offset_showing_from_below(40.)
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_newline_at_the_bottom_edge_scrolls_the_caret_back_into_view(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).line_for_offset(view.cursor()), 41);
            assert_eq!(
                view.scroll_handle.offset().y,
                geometry::offset_showing_from_below(41.)
            );
        })
        .unwrap();
}

/// Falsifies deriving the unscrolled top on demand as `last_bounds.origin.y -
/// scroll_handle.offset().y`: the second move's `last_bounds` predates the
/// first move's `set_offset`, so that formula would scroll twice as far.
#[gpui::test]
fn two_moves_before_a_repaint_scroll_once_each(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("down");
    }

    window
        .update(&mut cx, |view, window, cx| {
            view.down(&Down, window, cx);
            view.down(&Down, window, cx);
        })
        .unwrap();

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).line_for_offset(view.cursor()), 42);
            assert_eq!(
                view.scroll_handle.offset().y,
                geometry::offset_showing_from_below(42.)
            );
        })
        .unwrap();
}

#[gpui::test]
fn typing_after_a_wheel_scroll_brings_the_caret_back(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(gpui::px(17.), middle_of_line(2.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-1000.))),
        ..gpui::ScrollWheelEvent::default()
    });

    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.scroll_handle.offset().y < gpui::px(0.),
                "the wheel event did not scroll the document"
            );
        })
        .unwrap();

    cx.simulate_input("x");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.scroll_handle.offset().y, gpui::px(0.));
        })
        .unwrap();
}

#[gpui::test]
fn extending_a_selection_reveals_its_head(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-up");
    for _ in 0..40 {
        cx.simulate_keystrokes("shift-down");
    }

    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.state.selection().range().start, at(0));
            assert_eq!(
                view.scroll_handle.offset().y,
                geometry::offset_showing_from_below(40.)
            );
        })
        .unwrap();
}

/// The shrink case gpui's own clamp cannot cover: undoing moves the caret's
/// line index without the document's end moving under it.
#[gpui::test]
fn deleting_and_restoring_lines_moves_the_caret_both_ways(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down cmd-a backspace");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).line_count(), 1);
            assert_eq!(view.scroll_handle.offset().y, gpui::px(0.));
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            #[allow(clippy::cast_precision_loss)]
            let line = view.buffer.read(cx).line_for_offset(view.cursor()) as f32;
            let offset = view.scroll_handle.offset().y;
            assert!(offset < gpui::px(0.), "the undo did not scroll back down");
            assert!(
                geometry::line_is_visible(line, offset),
                "line {line} is at {:?} after the undo",
                geometry::top_of_line(line, offset)
            );
        })
        .unwrap();
}

/// `position_for_point` resolves against `last_bounds`, which is already
/// scroll-translated, while the click event's position is plain window space
/// — the one path where a coordinate-space mistake hides.
#[gpui::test]
fn a_click_while_scrolled_lands_on_the_line_under_the_pointer(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    let scrolled = window
        .update(&mut cx, |view, _, _| view.scroll_handle.offset().y)
        .unwrap();
    assert!(scrolled < gpui::px(0.));

    let y = gpui::px(geometry::VIEWPORT / 2.);
    let expected = geometry::line_at(y, scrolled);
    cx.simulate_click(gpui::point(gpui::px(17.), y), gpui::Modifiers::none());

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.buffer.read(cx).line_for_offset(view.cursor()),
                expected
            );
            assert_eq!(
                view.scroll_handle.offset().y,
                scrolled,
                "clicking a visible line scrolled the document"
            );
        })
        .unwrap();
}

/// Tall *and* wide on purpose: a wide-only document never scrolls, so the
/// assertion would hold vacuously against an implementation that writes no offset at all.
#[gpui::test]
fn revealing_never_scrolls_horizontally(cx: &mut TestAppContext) {
    let wide_and_tall = format!("{}\n", "x".repeat(2000)).repeat(TALL);
    let (window, mut cx) = dispatching(cx, &wide_and_tall);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    window
        .update(&mut cx, |view, _, _| {
            assert!(
                view.scroll_handle.offset().y < gpui::px(0.),
                "the document did not scroll vertically, so x was never written"
            );
            assert_eq!(view.scroll_handle.offset().x, gpui::px(0.));
            assert_eq!(
                view.scroll_handle.max_offset().x,
                gpui::px(0.),
                "a line overflowed"
            );
        })
        .unwrap();
}

/// `p_4()` padding puts the origin at (16, 16) with a 28px line height, so
/// line `n` occupies y in `[16 + 28n, 16 + 28(n+1))`. Aim at the middle of the
/// band, not its edge, so a boundary-rounding change doesn't break this.
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

/// A click below a short document arrives with a y past the last row;
/// unclamped, that indexes off the end of the layout.
#[gpui::test]
fn a_click_below_the_last_line_clamps_to_it(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "ab\ncd");
    cx.run_until_parked();

    // Move off the target first, or the assertion would hold even if the click never arrived.
    cx.simulate_keystrokes("cmd-up");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(0)))
        .unwrap();

    // Kept inside the 1920x1080 test window: a click outside it never hits the div's hitbox.
    cx.simulate_click(
        gpui::point(gpui::px(1800.), middle_of_line(5.)),
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

/// An edit reshapes in its gate, so geometry read before the next paint already describes the new text.
#[gpui::test]
fn an_edit_reshapes_before_the_next_paint(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "one");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, window, cx| {
            assert_eq!(view.last_layout.as_ref().unwrap().row_count(), 1);
            view.newline(&crate::editor::actions::Newline, window, cx);
            assert_eq!(
                view.last_layout.as_ref().unwrap().row_count(),
                2,
                "the layout must describe the text the edit left, not the painted one"
            );
        })
        .unwrap();
}
