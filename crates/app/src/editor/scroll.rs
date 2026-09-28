use std::ops::Range;

use gpui::{Context, Pixels, Window, px};

use super::EditorView;
use super::reload::TopLine;

const MARGIN_LINES: f32 = 3.;

/// How far a reveal scrolls: just enough, as a caret move does, or to the middle, as a search hit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reveal {
    Minimal,
    Center,
}

impl Reveal {
    pub(super) fn offset(
        self,
        line: Range<Pixels>,
        viewport: Range<Pixels>,
        offset: Pixels,
        line_height: Pixels,
    ) -> Option<Pixels> {
        match self {
            Self::Minimal => reveal(line, viewport, offset, line_height),
            Self::Center => center(line, viewport, offset),
        }
    }
}

/// A scroll decided before the geometry it needs; `render` applies it.
pub(super) enum ScrollTo {
    /// A reload's top line, back at the top once the new text is laid out.
    Top(TopLine),
    Cursor(Reveal),
}

impl EditorView {
    /// A reveal replaces any request; a reload's top line only fills an empty slot.
    pub(super) fn request_scroll(&mut self, to: ScrollTo) {
        if matches!(to, ScrollTo::Cursor(_)) || self.scroll_to.is_none() {
            self.scroll_to = Some(to);
        }
    }

    /// From `render`, before prepaint reads the offset, else the scroll lands a frame late.
    pub(super) fn restore_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(to) = self.scroll_to.take() else {
            return;
        };
        // A reload may have landed since the last paint shaped `last_layout`.
        self.reshape(window, cx);
        match to {
            ScrollTo::Top(top) => self.restore_top(&top),
            ScrollTo::Cursor(how) => self.reveal_cursor(how),
        }
    }
}

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

/// Offset that puts `line`'s middle at `viewport`'s, or `None` if it is there; clamped as `reveal`.
pub(super) fn center(
    line: Range<Pixels>,
    viewport: Range<Pixels>,
    offset: Pixels,
) -> Option<Pixels> {
    let target = ((viewport.start + viewport.end) - (line.start + line.end)) / 2.;
    let target = target.min(px(0.));
    (target != offset).then_some(target)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests;
