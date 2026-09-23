use std::ops::Range;

use gpui::{Pixels, px};

const MARGIN_LINES: f32 = 3.;

/// Offset that brings `line` into `viewport`, or `None` if already visible.
/// Unscrolled coordinates; only the upper bound is clamped — the lower is gpui's own job.
pub(super) fn reveal(
    line: Range<Pixels>,
    viewport: Range<Pixels>,
    offset: Pixels,
) -> Option<Pixels> {
    let (line_height, viewport_height) = (line.end - line.start, viewport.end - viewport.start);

    let margin =
        (line_height * MARGIN_LINES).min(((viewport_height - line_height) / 2.).max(px(0.)));

    let top = viewport.start + margin - line.start;
    let bottom = viewport.end - margin - line.end;

    let target = if top >= bottom {
        // Empty range (an overtall line, or a margin clamp landing an ulp
        // apart): resolve to top rather than alternate and jitter.
        top
    } else if offset < top {
        top
    } else if offset > bottom {
        bottom
    } else {
        return None;
    };

    // Not tidiness: the empty-range branch always returns a target, so
    // without this it would report a move even at the offset it's already at.
    let target = target.min(px(0.));
    (target != offset).then_some(target)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const LINE: f32 = 28.;
    const VIEWPORT: Range<f32> = 0. ..280.;

    fn viewport() -> Range<Pixels> {
        px(VIEWPORT.start)..px(VIEWPORT.end)
    }

    fn line(n: f32) -> Range<Pixels> {
        px(n * LINE)..px((n + 1.) * LINE)
    }

    #[test]
    fn a_visible_line_does_not_scroll() {
        assert_eq!(reveal(line(5.), viewport(), px(0.)), None);
    }

    #[test]
    fn a_line_above_the_viewport_comes_down_to_the_margin() {
        let offset = px(-20. * LINE);
        let revealed = reveal(line(10.), viewport(), offset).unwrap();
        assert_eq!(revealed, px(-(10. - 3.) * LINE));
    }

    #[test]
    fn a_line_below_the_viewport_comes_up_to_the_margin() {
        let revealed = reveal(line(20.), viewport(), px(0.)).unwrap();
        assert_eq!(revealed, px(VIEWPORT.end - 3. * LINE - 21. * LINE));
    }

    #[test]
    fn a_short_viewport_clamps_the_margin_instead_of_honouring_it() {
        let short = px(0.)..px(4. * LINE);
        let revealed = reveal(line(20.), short.clone(), px(0.)).unwrap();

        // (4 - 1) / 2 = 1.5 lines of margin, not the 3 asked for.
        assert_eq!(revealed, px(4. * LINE - 1.5 * LINE - 21. * LINE));
        let top = px(20. * LINE) + revealed;
        assert!(top >= short.start && top + px(LINE) <= short.end);
    }

    #[test]
    fn a_viewport_shorter_than_a_line_settles_instead_of_oscillating() {
        let cramped = px(0.)..px(10.);
        let first = reveal(line(20.), cramped.clone(), px(0.)).unwrap();
        assert_eq!(reveal(line(20.), cramped, first), None);
    }

    #[test]
    fn a_viewport_exactly_one_line_tall_has_one_valid_offset() {
        let exact = px(0.)..px(LINE);
        let revealed = reveal(line(20.), exact.clone(), px(0.)).unwrap();
        assert_eq!(revealed, px(-20. * LINE));
        assert_eq!(reveal(line(20.), exact, revealed), None);
    }

    #[test]
    fn revealing_near_the_start_stops_at_the_top() {
        let revealed = reveal(line(1.), viewport(), px(-10. * LINE)).unwrap();
        assert_eq!(revealed, px(0.));
    }

    proptest! {
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
