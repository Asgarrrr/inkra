use std::ops::Range;

use gpui::{App, Pixels, Point, TextAlign, Window, fill, point, px};

use super::DocumentLayout;
use super::prefix::Prefix;
use crate::theme;

#[cfg(test)]
thread_local! {
    static PAINTED: std::cell::RefCell<(Range<usize>, Range<usize>)> =
        const { std::cell::RefCell::new((0..0, 0..0)) };
}

/// The lines and bars the last paint on this thread walked.
#[cfg(test)]
pub(crate) fn painted() -> (Range<usize>, Range<usize>) {
    PAINTED.with(|painted| painted.borrow().clone())
}

impl DocumentLayout {
    /// One `WrappedLine::paint` per line in the content mask; gpui steps rows down at each wrap. A
    /// failed line is skipped and logged, not fatal, so one bad glyph run doesn't blank the page.
    pub(crate) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let mask = window.content_mask().bounds;
        let lines = self.visible_lines(mask.top() - origin.y, mask.bottom() - origin.y);
        let bars = self.bars_of(&lines);
        #[cfg(test)]
        PAINTED.with(|painted| *painted.borrow_mut() = (lines.clone(), bars.clone()));
        for bar in &self.bars[bars] {
            window.paint_quad(fill(*bar + origin, theme::quote_bar()));
        }
        for index in lines {
            let line = &self.lines[index];
            let line_origin = origin + point(px(0.), line.top);
            if let Err(err) = line.shape.wrapped.paint(
                line_origin + point(line.indent(), px(0.)),
                line.row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::warn!("failed to paint line {index}: {err}");
            }
            if let Some((shaped, x)) = line.prefix.as_ref().and_then(Prefix::shown)
                && let Err(err) = shaped.paint(
                    line_origin + point(x, px(0.)),
                    line.row_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
            {
                log::warn!("failed to paint the prefix of line {index}: {err}");
            }
        }
    }
    /// The lines reaching `top..bottom` in layout space, one more each side: a glyph can overhang its row.
    fn visible_lines(&self, top: Pixels, bottom: Pixels) -> Range<usize> {
        let start = self
            .lines
            .partition_point(|line| line.top <= top)
            .saturating_sub(2);
        let end = (self.lines.partition_point(|line| line.top < bottom) + 1).min(self.lines.len());
        start..end.max(start)
    }

    /// The bars starting within `lines`' span; `bars` is sorted by top.
    fn bars_of(&self, lines: &Range<usize>) -> Range<usize> {
        let from = self.lines[lines.start].top;
        let to = self
            .lines
            .get(lines.end)
            .map_or(self.height, |line| line.top);
        self.bars.partition_point(|bar| bar.origin.y < from)
            ..self.bars.partition_point(|bar| bar.origin.y < to)
    }
}
