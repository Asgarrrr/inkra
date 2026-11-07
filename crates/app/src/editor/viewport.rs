//! Where the editor's text sits under its scroll, and the scroll waiting for a layout.

use std::ops::Range;

use gpui::{Pixels, ScrollHandle, point, px};
use inkra_text::{Anchor, Bias, Buffer, ByteOffset, Diff};

use super::layout::DocumentLayout;

const MARGIN_LINES: f32 = 3.;

/// How far a reveal scrolls: just enough, as a caret move does, or to the middle, as a search hit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reveal {
    Minimal,
    Center,
}

impl Reveal {
    fn offset(
        self,
        line: Range<Pixels>,
        viewport: Range<Pixels>,
        offset: Pixels,
        line_height: Pixels,
    ) -> Option<Pixels> {
        match self {
            Self::Minimal => minimal(line, viewport, offset, line_height),
            Self::Center => center(line, viewport, offset),
        }
    }
}

/// A scroll decided before the geometry it needs; `resolve` applies it in prepaint.
#[derive(Debug, PartialEq)]
pub(super) enum ScrollTo {
    /// A reload's or a restored view's top line, back at the top once the text is laid out.
    Top(TopLine),
    Cursor(Reveal),
}

impl ScrollTo {
    /// The caret needs no mapping here: `map_through` already moved it.
    fn mapped(self, diff: &Diff) -> Self {
        match self {
            Self::Top(top) => Self::Top(top.mapped(diff)),
            cursor @ Self::Cursor(_) => cursor,
        }
    }
}

/// The text at the viewport's top, to put back there once the new text is laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TopLine {
    /// The top row's start, or the caret while a reveal waits.
    offset: ByteOffset,
    /// How far the viewport's top sits below that row's top.
    below: Pixels,
}

impl TopLine {
    fn mapped(self, diff: &Diff) -> Self {
        Self {
            offset: diff.map(self.offset, Bias::Left),
            ..self
        }
    }

    pub(super) fn clipped(self, buffer: &Buffer) -> Self {
        Self {
            offset: buffer.clip_grapheme(self.offset, Bias::Left),
            ..self
        }
    }

    pub(super) fn at(offset: ByteOffset) -> Self {
        Self {
            offset,
            below: px(0.),
        }
    }

    /// `Left`, as `mapped`: the offset must be a char boundary within `buffer`.
    pub(super) fn park(self, buffer: &mut Buffer) -> ParkedTop {
        ParkedTop {
            offset: buffer.anchor(self.offset, Bias::Left),
            below: self.below,
        }
    }
}

/// A `TopLine` kept as an anchor while its view is not showing.
#[derive(Debug)]
pub(super) struct ParkedTop {
    offset: Anchor,
    below: Pixels,
}

impl ParkedTop {
    /// `below` is kept: restoring bounds it by the row's height in the new layout.
    pub(super) fn unpark(self, buffer: &mut Buffer) -> TopLine {
        let top = self.peek(buffer);
        buffer.forget(self.offset);
        top
    }

    /// As `unpark`, forgetting nothing.
    pub(super) fn peek(&self, buffer: &Buffer) -> TopLine {
        TopLine {
            offset: buffer.resolve(self.offset),
            below: self.below,
        }
    }
}

impl TopLine {
    pub(crate) fn new(offset: ByteOffset, below: Pixels) -> Self {
        Self { offset, below }
    }

    pub(crate) fn offset(&self) -> ByteOffset {
        self.offset
    }

    pub(crate) fn below(&self) -> Pixels {
        self.below
    }
}

/// What `resolve` did this frame.
pub(super) struct Resolution {
    /// How far the offset moved.
    pub(super) moved: Pixels,
    /// A waiting reveal was applied, so the caret's block still needs its own.
    pub(super) revealed: bool,
}

#[derive(Default)]
pub(super) struct Viewport {
    /// Its `bounds` and `offset` are the viewport's; the parent `div` scrolls, not the element.
    handle: ScrollHandle,
    /// Text top at zero scroll, written only by `resolve`; `last_bounds` plus the offset would count the scroll twice.
    text_top: Option<Pixels>,
    pending: Option<ScrollTo>,
}

impl Viewport {
    pub(super) fn handle(&self) -> &ScrollHandle {
        &self.handle
    }

    /// From prepaint, against this frame's measured layout.
    pub(super) fn resolve(
        &mut self,
        text_origin_y: Pixels,
        layout: &DocumentLayout,
        cursor: ByteOffset,
    ) -> Resolution {
        // The offset now is what translated this frame's origin (`elements/div.rs:2027`).
        let before = self.handle.offset().y;
        self.text_top = Some(text_origin_y - before);
        let Some(to) = self.pending.take() else {
            return Resolution {
                moved: px(0.),
                revealed: false,
            };
        };
        let revealed = self.apply(&to, Some(layout), cursor);
        // This frame's clamp already ran; unclamped, the next one would jump.
        let after = self.handle.offset().y.max(-self.handle.max_offset().y);
        self.set_y(after);
        Resolution {
            moved: after - before,
            revealed,
        }
    }

    /// A reveal replaces any request; a reload's top line only fills an empty slot.
    pub(super) fn request(&mut self, to: ScrollTo) {
        if matches!(to, ScrollTo::Cursor(_)) || self.pending.is_none() {
            self.pending = Some(to);
        }
    }

    #[cfg(test)]
    pub(super) fn take_pending(&mut self) -> Option<ScrollTo> {
        self.pending.take()
    }

    /// Scrolls now when the geometry is current, else requests it for `resolve` in the next
    /// prepaint; true when the geometry was current, moved or not.
    pub(super) fn reveal(
        &mut self,
        layout: Option<&DocumentLayout>,
        cursor: ByteOffset,
        how: Reveal,
    ) -> bool {
        let viewport = self.handle.bounds();
        // Bounds stay default() until the first prepaint; a waiting request means a stale layout.
        let geometry = self
            .text_top
            .zip(layout)
            .filter(|_| viewport.size.height > px(0.) && self.pending.is_none());
        let Some((text_top, layout)) = geometry else {
            self.request(ScrollTo::Cursor(how));
            return false;
        };

        let row = layout.row_for_offset(cursor);
        let top = text_top + layout.top_for_row(row);
        let extent = top..top + layout.row_height(row);
        let viewport = viewport.top()..viewport.bottom();

        if let Some(offset) = how.offset(
            extent,
            viewport,
            self.handle.offset().y,
            layout.body_row_height(),
        ) {
            self.set_y(offset);
        }
        true
    }

    /// `layout` must describe the current text; true when it applied a reveal.
    fn apply(
        &mut self,
        to: &ScrollTo,
        layout: Option<&DocumentLayout>,
        cursor: ByteOffset,
    ) -> bool {
        match to {
            ScrollTo::Top(top) => {
                self.restore_top(layout, top);
                false
            }
            ScrollTo::Cursor(how) => self.reveal(layout, cursor, *how),
        }
    }

    /// What a capture keeps: a waiting request first, since the drawn frame predates it.
    pub(super) fn capture(&self, layout: Option<&DocumentLayout>, cursor: ByteOffset) -> TopLine {
        match &self.pending {
            Some(ScrollTo::Top(top)) => *top,
            Some(ScrollTo::Cursor(_)) => TopLine::at(cursor),
            None => self.top_line(layout).unwrap_or_default(),
        }
    }

    /// A change from disk: requests the old top line, then maps any waiting request through `diff`.
    pub(super) fn reloaded(&mut self, diff: &Diff, layout: Option<&DocumentLayout>) {
        let fresh = self.top_line(layout);
        self.follow(diff, fresh);
    }

    /// Request, then map: a waiting top line is already in the text before `diff`, so it wins.
    fn follow(&mut self, diff: &Diff, fresh: Option<TopLine>) {
        if let Some(top) = fresh {
            self.request(ScrollTo::Top(top));
        }
        self.pending = self.pending.take().map(|to| to.mapped(diff));
    }

    /// The top line in the text `layout` describes; `None` unscrolled.
    fn top_line(&self, layout: Option<&DocumentLayout>) -> Option<TopLine> {
        let offset = self.handle.offset().y;
        if offset == px(0.) {
            return None;
        }
        let layout = layout?;
        let y = self.viewport_top()? - offset;
        let row = layout.row_for_offset(layout.offset_for_position(point(px(0.), y)));
        Some(TopLine {
            offset: *layout.row_bounds(row).start(),
            below: y - layout.top_for_row(row),
        })
    }

    fn restore_top(&self, layout: Option<&DocumentLayout>, top: &TopLine) {
        let (Some(layout), Some(viewport_top)) = (layout, self.viewport_top()) else {
            return;
        };
        let row = layout.row_for_offset(top.offset);
        // Within its own row: one that got shorter (a heading now body text) would overshoot into the next.
        let below = top.below.min(layout.row_height(row));
        let y = viewport_top - (layout.top_for_row(row) + below);
        self.set_y(y.min(px(0.)));
    }

    /// Never scrolls horizontally.
    fn set_y(&self, y: Pixels) {
        self.handle.set_offset(point(self.handle.offset().x, y));
    }

    /// The viewport's top in the text's own y at zero scroll; `None` before the first paint.
    fn viewport_top(&self) -> Option<Pixels> {
        Some(self.handle.bounds().top() - self.text_top?)
    }
}

#[cfg(test)]
impl Viewport {
    fn drawn_top(&self) -> Pixels {
        let Some(viewport_top) = self.viewport_top() else {
            panic!("the editor has not been drawn");
        };
        viewport_top
    }

    /// Scrolls so the row holding `offset` starts at the viewport's top.
    pub(super) fn scroll_row_to_top(&self, layout: &DocumentLayout, offset: ByteOffset) {
        let row_top = layout.top_for_row(layout.row_for_offset(offset));
        self.handle
            .set_offset(point(px(0.), self.drawn_top() - row_top));
    }

    /// The offset at the viewport's top-left.
    pub(super) fn offset_at_top(&self, layout: &DocumentLayout) -> ByteOffset {
        let y = self.drawn_top() - self.handle.offset().y;
        layout.offset_for_position(point(px(0.), y))
    }

    /// Whether the whole row holding `offset` lies inside the viewport.
    pub(super) fn shows_row(&self, layout: &DocumentLayout, offset: ByteOffset) -> bool {
        let top = self.drawn_top() - self.handle.offset().y;
        let bottom = top + self.handle.bounds().size.height;
        let row = layout.row_for_offset(offset);
        let row_top = layout.top_for_row(row);
        top <= row_top && row_top + layout.row_height(row) <= bottom
    }
}

/// Offset that brings `line` into `viewport`, or `None` if already visible.
/// Unscrolled coordinates; only the upper bound is clamped — the lower is gpui's own job.
/// The margin counts lines of `line_height`, as Zed's `vertical_scroll_margin` does.
fn minimal(
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

/// Offset that puts `line`'s middle at `viewport`'s, or `None` if it is there; clamped as `minimal`.
fn center(line: Range<Pixels>, viewport: Range<Pixels>, offset: Pixels) -> Option<Pixels> {
    let target = ((viewport.start + viewport.end) - (line.start + line.end)) / 2.;
    let target = target.min(px(0.));
    (target != offset).then_some(target)
}

#[cfg(test)]
mod tests;
