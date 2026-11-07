//! Each code block's horizontal scroll (code block scroll spec, D1): one table the editor writes and
//! every layout it shapes reads, so a wheel tick changes the paint without a reshape.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Bounds, Pixels, point, px, size};
use inkra_text::{Anchor, Bias, Buffer, ByteOffset};

use super::{DocumentLayout, NEWLINE_STUB};
use crate::theme;

#[derive(Clone, Default)]
pub(crate) struct BlockScrolls(Rc<RefCell<Vec<BlockScroll>>>);

struct BlockScroll {
    /// `Bias::Right`: a line typed at the block's start keeps the anchor on the fence.
    anchor: Anchor,
    /// `anchor` resolved at the last `sync`; sorted by it.
    start: ByteOffset,
    x: Pixels,
    /// The last time `x` changed by a scroll: the bar lingers from it.
    scrolled_at: Instant,
}

impl BlockScrolls {
    /// The block starting at `start`; zero for a block never scrolled.
    pub(crate) fn x(&self, start: ByteOffset) -> Pixels {
        let scrolls = self.0.borrow();
        scrolls
            .binary_search_by_key(&start, |scroll| scroll.start)
            .map_or(px(0.), |index| scrolls[index].x)
    }

    /// When block `start` last scrolled; `None` for a block never scrolled.
    pub(crate) fn scrolled_at(&self, start: ByteOffset) -> Option<Instant> {
        let scrolls = self.0.borrow();
        scrolls
            .binary_search_by_key(&start, |scroll| scroll.start)
            .ok()
            .map(|index| scrolls[index].scrolled_at)
    }

    /// An entry set to zero stays, keeping its anchor; a missing one is added only for a nonzero `x`.
    /// Stamps `now`: every caller passes a changed `x`.
    pub(crate) fn set(&self, start: ByteOffset, x: Pixels, now: Instant, buffer: &mut Buffer) {
        let mut scrolls = self.0.borrow_mut();
        match scrolls.binary_search_by_key(&start, |scroll| scroll.start) {
            Ok(index) => {
                scrolls[index].x = x;
                scrolls[index].scrolled_at = now;
            }
            Err(index) if x != px(0.) => {
                let anchor = buffer.anchor(start, Bias::Right);
                let scroll = BlockScroll {
                    anchor,
                    start,
                    x,
                    scrolled_at: now,
                };
                scrolls.insert(index, scroll);
            }
            _ => {}
        }
    }

    /// Re-reads each start, keeping only block starts and the first entry per start; returns the
    /// dropped anchors for the caller to forget.
    pub(crate) fn sync(
        &self,
        buffer: &Buffer,
        is_start: impl Fn(ByteOffset) -> bool,
    ) -> Vec<Anchor> {
        let mut scrolls = self.0.borrow_mut();
        for scroll in scrolls.iter_mut() {
            scroll.start = buffer.resolve(scroll.anchor);
        }
        scrolls.sort_by_key(|scroll| scroll.start);
        let mut dropped = Vec::new();
        let mut kept = None;
        scrolls.retain(|scroll| {
            let keep = kept != Some(scroll.start) && is_start(scroll.start);
            if keep {
                kept = Some(scroll.start);
            } else {
                dropped.push(scroll.anchor);
            }
            keep
        });
        dropped
    }

    pub(crate) fn forget_all(&self, buffer: &mut Buffer) {
        for scroll in self.0.borrow_mut().drain(..) {
            buffer.forget(scroll.anchor);
        }
    }

    /// Every scrolled block, as plain offsets for a `ViewState`.
    pub(crate) fn entries(&self) -> Vec<(ByteOffset, Pixels)> {
        self.0
            .borrow()
            .iter()
            .filter(|scroll| scroll.x != px(0.))
            .map(|scroll| (scroll.start, scroll.x))
            .collect()
    }

    /// Onto an empty table: an entry left from another view would leak its anchor.
    pub(crate) fn anchor_all(
        &self,
        entries: &[(ByteOffset, Pixels)],
        now: Instant,
        buffer: &mut Buffer,
    ) {
        debug_assert!(self.is_empty(), "anchoring over a live table");
        for &(start, x) in entries {
            self.set(start, x, now, buffer);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    #[cfg(test)]
    pub(crate) fn anchors(&self) -> Vec<Anchor> {
        self.0.borrow().iter().map(|scroll| scroll.anchor).collect()
    }
}

/// Where a scrollable block's text is cut, in layout coordinates: left only once it scrolled.
pub(super) struct Clip {
    pub(super) left: Option<Pixels>,
    pub(super) right: Pixels,
}

impl Clip {
    /// A quad's span on a clipped row: left of the background it becomes a stub at its edge, so a
    /// selected blank line still shows; right of it, nothing.
    pub(super) fn span(
        &self,
        left: Pixels,
        right: Pixels,
        row_height: Pixels,
    ) -> Option<(Pixels, Pixels)> {
        if let Some(edge) = self.left
            && right <= edge
        {
            return Some((edge, edge + row_height * NEWLINE_STUB));
        }
        if left >= self.right {
            return None;
        }
        let left = self.left.map_or(left, |edge| left.max(edge));
        Some((left, right.min(self.right)))
    }
}

/// A caret's block, in layout coordinates before its scroll (code block scroll spec, D5).
pub(crate) struct BlockSpan {
    pub(crate) start: ByteOffset,
    /// The caret's content x.
    pub(crate) caret: Pixels,
    /// The text's left edge.
    pub(crate) left: Pixels,
    /// The text's width inside the background.
    pub(crate) visible: Pixels,
    pub(crate) max: Pixels,
}

impl DocumentLayout {
    /// The scrollable block holding `offset`; `None` outside a block or in one that fits.
    pub(crate) fn block_span(&self, offset: ByteOffset) -> Option<BlockSpan> {
        let (line, within) = self.locate(self.row_for_offset(offset));
        let block = &self.code_blocks[line.block?];
        (block.max > px(0.)).then(|| BlockSpan {
            start: block.start,
            caret: Self::content_x(line, within, offset),
            left: block.background.left() + theme::CODE_BLOCK_OUTSET,
            visible: block.background.size.width - theme::CODE_BLOCK_OUTSET * 2.,
            max: block.max,
        })
    }

    /// Block `index`'s scroll, clamped to its current `max`: an edit or a resize can shrink it.
    pub(crate) fn block_scroll(&self, index: usize) -> Pixels {
        let block = &self.code_blocks[index];
        self.scrolls.x(block.start).max(px(0.)).min(block.max)
    }

    /// Block `index`'s position bar inside its background's bottom edge; `None` when it fits.
    pub(crate) fn scroll_bar(&self, index: usize) -> Option<Bounds<Pixels>> {
        let block = &self.code_blocks[index];
        (block.max > px(0.)).then(|| {
            let visible = block.background.size.width - theme::CODE_BLOCK_OUTSET * 2.;
            let total = visible + block.max;
            let left = block.background.left()
                + theme::CODE_BLOCK_OUTSET
                + self.block_scroll(index) * (visible / total);
            let top = block.background.bottom()
                - theme::CODE_BLOCK_BAR_INSET
                - theme::CODE_BLOCK_BAR_HEIGHT;
            Bounds::new(
                point(left, top),
                size(visible * (visible / total), theme::CODE_BLOCK_BAR_HEIGHT),
            )
        })
    }

    pub(super) fn block_clip(&self, index: usize) -> Option<Clip> {
        let block = &self.code_blocks[index];
        (block.max > px(0.)).then(|| Clip {
            left: (self.block_scroll(index) > px(0.)).then_some(block.background.left()),
            right: block.background.right(),
        })
    }

    /// Each block in `top..bottom` that can scroll: its index, start, `max` and background.
    pub(crate) fn scrollable_blocks(
        &self,
        top: Pixels,
        bottom: Pixels,
    ) -> impl Iterator<Item = (usize, ByteOffset, Pixels, Bounds<Pixels>)> + '_ {
        let range = self.blocks_in(top, bottom);
        self.code_blocks[range.clone()]
            .iter()
            .zip(range)
            .filter(|(block, _)| block.max > px(0.))
            .map(|(block, index)| (index, block.start, block.max, block.background))
    }

    /// False when the caret at `offset` lies outside its scrolled block's background.
    pub(crate) fn caret_visible(&self, offset: ByteOffset) -> bool {
        let line = &self.lines[self.line_for_offset(offset)];
        let Some(clip) = line.block.and_then(|block| self.block_clip(block)) else {
            return true;
        };
        let x = self.caret_bounds(offset).left();
        !(clip.left.is_some_and(|left| x < left) || x >= clip.right)
    }
}

#[cfg(test)]
mod tests;
