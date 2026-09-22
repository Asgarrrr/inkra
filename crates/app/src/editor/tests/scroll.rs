//! Scroll geometry, reveal-on-move, and click-to-offset hit-testing.

use gpui::TestAppContext;

use super::{at, dispatching};
use crate::editor::actions::Down;

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

/// The geometry the reveal tests assert against, spelled out once.
///
/// The test display is 1920x1080 (`gpui/platform/test/display.rs:16`) — not
/// `DEFAULT_WINDOW_SIZE`, which is what a real window opens at and is 15px
/// taller.
///
/// Written out rather than read back from the view: derived expectations would
/// restate the implementation, so a change to the padding or the line height
/// would pass quietly instead of failing here.
mod geometry {
    pub(super) const TEXT_TOP: f32 = 16.;
    pub(super) const LINE_HEIGHT: f32 = 28.;
    pub(super) const VIEWPORT: f32 = 1080.;
    /// `MARGIN_LINES` from `scroll.rs`, in pixels. The viewport is far taller
    /// than seven lines, so the clamp never binds in these tests.
    pub(super) const MARGIN: f32 = 3. * LINE_HEIGHT;

    /// The offset at which line `n`'s bottom sits one margin above the
    /// viewport's bottom — what revealing a line from above settles on.
    pub(super) fn offset_showing_from_below(n: f32) -> gpui::Pixels {
        gpui::px(VIEWPORT - MARGIN - (TEXT_TOP + LINE_HEIGHT * (n + 1.)))
    }

    /// Line `n`'s top in window coordinates, at the given scroll offset.
    pub(super) fn top_of_line(n: f32, offset: gpui::Pixels) -> gpui::Pixels {
        gpui::px(TEXT_TOP + LINE_HEIGHT * n) + offset
    }

    /// The line a click at window `y` lands on, at the given scroll offset —
    /// the inverse of [`top_of_line`].
    pub(super) fn line_at(y: gpui::Pixels, offset: gpui::Pixels) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let line = f32::from(y - offset - gpui::px(TEXT_TOP)) / LINE_HEIGHT;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let line = line as usize;
        line
    }

    /// Whether line `n` is wholly inside the viewport at the given offset.
    pub(super) fn line_is_visible(n: f32, offset: gpui::Pixels) -> bool {
        let top = top_of_line(n, offset);
        top >= gpui::px(0.) && top + gpui::px(LINE_HEIGHT) <= gpui::px(VIEWPORT)
    }
}

/// Longer than the viewport by about five screens.
const TALL: usize = 200;

fn tall_document() -> String {
    "line\n".repeat(TALL)
}

/// The caret starts at the end of the buffer, so the first paint already has it
/// off screen. Reaching the last line must bring it back.
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

/// Nothing else here would notice the margin going missing: a reveal without
/// one leaves the caret flush against the bottom edge and still satisfies every
/// `line_is_visible` assertion.
///
/// The viewport holds lines 0 to 37 and the margin starts pushing from line 35,
/// so line 40 is well past the edge.
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

/// The defect the roadmap named: typing past the bottom edge used to lose the
/// caret.
///
/// The assertion is the exact offset, not just "visible", because the margin is
/// wide enough to hide an off-by-one from a bare visibility check. A reveal
/// taking the line index from `DocumentLayout` rather than the buffer would be
/// exactly that: it describes the previous frame, which has one line fewer.
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

/// Two moves inside one `update`, so only one frame is drawn — at the end.
///
/// This is the one test that falsifies deriving the text's unscrolled top on
/// demand as `last_bounds.origin.y - scroll_handle.offset().y`. The second
/// `down` runs against a `last_bounds` that predates the first one's
/// `set_offset`, so that formula would subtract an offset those bounds never
/// saw and scroll twice as far.
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

/// A real wheel event, not a `set_offset` standing in for one: the div installs
/// its own handler whenever it scrolls (`paint_scroll_listener`), and it only
/// fires for a position inside the hitbox, so this exercises hit-testing too.
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

    // The caret never moved, so it is now above the viewport. Typing must
    // bring it back, which is the reveal running upwards rather than down.
    cx.simulate_input("x");
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.scroll_handle.offset().y, gpui::px(0.));
        })
        .unwrap();
}

/// Extending a selection moves its head, and it is the head that has to stay on
/// screen — the anchor is free to scroll away.
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

/// The shrink case gpui's own clamp cannot cover. Deleting at the very end
/// keeps the caret pinned to the bottom by the clamp alone, whatever we do;
/// deleting everything and undoing moves the caret's line index without the
/// document's end moving under it, in both directions.
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

    // Restores 200 lines above the caret, putting it far below the viewport.
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

/// A click while scrolled is the one path where a coordinate-space mistake
/// hides: `position_for_point` resolves against `last_bounds`, which is already
/// scroll-translated, while the event's position is plain window space.
///
/// The offset must not move either. The clicked line is visible by definition,
/// so a reveal that fired here would mean the geometry disagrees with the hit
/// test about where that line is.
#[gpui::test]
fn a_click_while_scrolled_lands_on_the_line_under_the_pointer(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, &tall_document());
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-down");
    let scrolled = window
        .update(&mut cx, |view, _, _| view.scroll_handle.offset().y)
        .unwrap();
    assert!(scrolled < gpui::px(0.));

    // Mid-viewport, so the target is nowhere near either edge.
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

/// Horizontal scrolling is deliberately out of scope, and the caret going off
/// the right edge of a long line is a known gap. This fails if a later change
/// wires up the x axis without the width measurement it needs.
///
/// Tall *and* wide on purpose: a wide document alone never scrolls, so the
/// assertion would hold against an implementation that writes no offset at all.
/// Being tall forces a real write, which is what proves it preserves x.
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
        })
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
    // inside the test window (1920x1080, `gpui/platform/test/display.rs:16`):
    // a click outside
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
