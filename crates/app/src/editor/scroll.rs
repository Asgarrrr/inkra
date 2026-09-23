use std::ops::Range;

use gpui::{Pixels, px};

const MARGIN_LINES: f32 = 3.;

/// Offset that brings `line` into `viewport`, or `None` if already visible.
/// Unscrolled coordinates; only the upper bound is clamped — the lower is gpui's own job.
/// The margin counts lines of `line_height`, as Zed's `vertical_scroll_margin` does.
pub(super) fn reveal(
    line: Range<Pixels>,
    viewport: Range<Pixels>,
    offset: Pixels,
    line_height: Pixels,
) -> Option<Pixels> {
    let (row_height, viewport_height) = (line.end - line.start, viewport.end - viewport.start);

    let margin =
        (line_height * MARGIN_LINES).min(((viewport_height - row_height) / 2.).max(px(0.)));

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
mod tests;
