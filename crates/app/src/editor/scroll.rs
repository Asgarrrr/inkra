//! Scrolling a line back into view.
//!
//! The arithmetic is here, alone and without a `Window`, so the awkward cases
//! can be tested directly rather than through a keystroke. What drives it is
//! [`super::EditorView::reveal_line`].

use std::ops::Range;

use gpui::{Pixels, px};

/// Lines of context kept above and below the revealed line, when the viewport
/// can spare them.
///
/// A constant rather than a setting: Zed's equivalent is configurable
/// (`editor/src/editor_settings.rs:36`) because Zed has a settings system, and
/// inventing one here to hold a single number would be the larger change.
const MARGIN_LINES: f32 = 3.;

/// The scroll offset that brings `line` into `viewport`, or `None` when it is
/// already there.
///
/// Both ranges are in window coordinates, unscrolled: `line` is where the line
/// would sit at offset zero, and `viewport` is the scroll container, which the
/// offset never moves. `offset` is gpui's, so it is zero at the top and
/// negative further down (`gpui/elements/div.rs:4391-4393`).
///
/// Minimal, like Zed's `Fit` (`editor/src/scroll/autoscroll.rs:252-263`): a line
/// that is already visible does not move, and one that is not moves just far
/// enough. Only the upper bound is clamped here. The lower one belongs to
/// gpui's `clamp_scroll_position` (`gpui/elements/div.rs:2479`), which runs
/// after the frame's new content size is known — clamping against
/// `max_offset()` here would stop a line short every time an edit added one.
pub(super) fn reveal(
    line: Range<Pixels>,
    viewport: Range<Pixels>,
    offset: Pixels,
) -> Option<Pixels> {
    let (line_height, viewport_height) = (line.end - line.start, viewport.end - viewport.start);

    // Clamped against the room there is, not used raw: three lines above and
    // below do not fit in a five-line viewport. Zed clamps the same way and
    // tests it (`editor/src/editor_tests.rs:3778-3796`), but in whole lines,
    // because its scroll position is a display row plus a fraction
    // (`editor/src/scroll.rs:35-38`). Ours is raw pixels, so flooring buys
    // nothing.
    let margin =
        (line_height * MARGIN_LINES).min(((viewport_height - line_height) / 2.).max(px(0.)));

    // The ends of the range of offsets that show the line, margin included.
    // Below `top` it has gone off the top of the viewport; above `bottom`, off
    // the bottom.
    let top = viewport.start + margin - line.start;
    let bottom = viewport.end - margin - line.end;

    let target = if top >= bottom {
        // An empty range: "above" and "below" are both true, and alternating
        // between them never settles, so the document would jitter on every
        // keystroke. Two ways in, both wanting the top.
        //
        // A line taller than the viewport cannot be shown whole, and the top is
        // the half to show, because the caret paints at the line's top
        // (`position_for_offset`).
        //
        // The other is one f32 rounding error wide: a margin clamp that binds
        // exactly makes the two ends one number in real arithmetic, but they
        // are computed by different routes and can land an ulp apart.
        top
    } else if offset < top {
        top
    } else if offset > bottom {
        bottom
    } else {
        return None;
    };

    // Not just tidiness: the empty-range branch returns a target
    // unconditionally, so without this it would report a move on every
    // keystroke to where the offset already is.
    let target = target.min(px(0.));
    (target != offset).then_some(target)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A 10-line viewport at the top of the window, and 28px lines — the app's
    /// real line height.
    const LINE: f32 = 28.;
    const VIEWPORT: Range<f32> = 0. ..280.;

    fn viewport() -> Range<Pixels> {
        px(VIEWPORT.start)..px(VIEWPORT.end)
    }

    /// The nth line, as `reveal` wants it: unscrolled window coordinates.
    fn line(n: f32) -> Range<Pixels> {
        px(n * LINE)..px((n + 1.) * LINE)
    }

    #[test]
    fn a_visible_line_does_not_scroll() {
        assert_eq!(reveal(line(5.), viewport(), px(0.)), None);
    }

    /// Ten lines of viewport leaves room for the full three-line margin.
    #[test]
    fn a_line_above_the_viewport_comes_down_to_the_margin() {
        // Scrolled so line 20 is at the top; revealing line 10 must put three
        // lines of context above it.
        let offset = px(-20. * LINE);
        let revealed = reveal(line(10.), viewport(), offset).unwrap();
        assert_eq!(revealed, px(-(10. - 3.) * LINE));
    }

    #[test]
    fn a_line_below_the_viewport_comes_up_to_the_margin() {
        // Line 20's bottom must land three lines above the viewport's bottom.
        let revealed = reveal(line(20.), viewport(), px(0.)).unwrap();
        assert_eq!(revealed, px(VIEWPORT.end - 3. * LINE - 21. * LINE));
    }

    /// The margin asked for is three lines; a four-line viewport cannot give
    /// three above *and* three below. It gives what it has rather than
    /// thrashing — Zed's `test_autoscroll_high_margin_keeps_cursor_centered`
    /// makes the same assertion at `editor/src/editor_tests.rs:3778-3796`.
    #[test]
    fn a_short_viewport_clamps_the_margin_instead_of_honouring_it() {
        let short = px(0.)..px(4. * LINE);
        let revealed = reveal(line(20.), short.clone(), px(0.)).unwrap();

        // (4 - 1) / 2 = 1.5 lines of margin, so the line's bottom sits 1.5 lines
        // above the viewport's bottom rather than the 3 that were asked for.
        assert_eq!(revealed, px(4. * LINE - 1.5 * LINE - 21. * LINE));
        // And it really is revealed: the line sits inside the viewport.
        let top = px(20. * LINE) + revealed;
        assert!(top >= short.start && top + px(LINE) <= short.end);
    }

    /// A viewport too short to hold one line has no offset that satisfies both
    /// edges, so the naive three-way test alternates forever and the document
    /// jitters on every keystroke. This is what pins the fixed point.
    #[test]
    fn a_viewport_shorter_than_a_line_settles_instead_of_oscillating() {
        let cramped = px(0.)..px(10.);
        let first = reveal(line(20.), cramped.clone(), px(0.)).unwrap();
        assert_eq!(reveal(line(20.), cramped, first), None);
    }

    /// The boundary case of the one above: exactly one line fits, the margin is
    /// zero, and the two edges agree on a single offset.
    #[test]
    fn a_viewport_exactly_one_line_tall_has_one_valid_offset() {
        let exact = px(0.)..px(LINE);
        let revealed = reveal(line(20.), exact.clone(), px(0.)).unwrap();
        assert_eq!(revealed, px(-20. * LINE));
        assert_eq!(reveal(line(20.), exact, revealed), None);
    }

    /// Revealing the first lines of a document asks for a margin the document
    /// cannot give, because there is nothing above line 0. That resolves to the
    /// top rather than to a positive offset, which gpui would clamp anyway.
    #[test]
    fn revealing_near_the_start_stops_at_the_top() {
        let revealed = reveal(line(1.), viewport(), px(-10. * LINE)).unwrap();
        assert_eq!(revealed, px(0.));
    }

    proptest! {
        /// Two invariants at once, over any geometry.
        ///
        /// A positive offset would scroll above the start of the document.
        /// A second call that answers anything but `None` means the first
        /// answer was not a fixed point — the general form of the oscillation
        /// the cramped-viewport test pins at one specific size, and of the
        /// one-ulp version that only shows up on a margin clamp that binds
        /// exactly. Random sizes find both; hand-picked ones found neither.
        #[test]
        fn revealing_settles_and_never_scrolls_past_the_start(
            line_top in 0f32..10_000.,
            line_height in 1f32..200.,
            viewport_top in 0f32..2_000.,
            viewport_height in 0f32..2_000.,
            offset in -10_000f32..=0.,
        ) {
            let line = px(line_top)..px(line_top + line_height);
            let viewport = px(viewport_top)..px(viewport_top + viewport_height);

            let Some(revealed) = reveal(line.clone(), viewport.clone(), px(offset)) else {
                return Ok(());
            };
            prop_assert!(revealed <= px(0.), "scrolled above the document start: {revealed:?}");
            prop_assert_eq!(reveal(line, viewport, revealed), None, "did not settle");
        }
    }
}
