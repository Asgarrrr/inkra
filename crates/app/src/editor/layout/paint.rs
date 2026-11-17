use std::ops::Range;

use gpui::{
    App, BorderStyle, Bounds, ContentMask, Pixels, Point, ShapedLine, TextAlign,
    TransformationMatrix, Window, fill, outline, point, px, size,
};
use inkra_text::{ByteOffset, Diff};

use super::DocumentLayout;
use super::line::LineLayout;
use super::prefix::Prefix;
use crate::theme;
use crate::ui::IconName;

#[cfg(test)]
thread_local! {
    static PAINTED: std::cell::RefCell<(Range<usize>, Range<usize>)> =
        const { std::cell::RefCell::new((0..0, 0..0)) };
    static PAINTED_BLOCKS: std::cell::RefCell<Range<usize>> =
        const { std::cell::RefCell::new(0..0) };
    static PAINTED_CLIPS: std::cell::RefCell<Vec<(Pixels, Bounds<Pixels>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PAINTED_TOPS: std::cell::RefCell<Vec<Pixels>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PAINTED_BOXES: std::cell::RefCell<Vec<PaintedBox>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A task box the last paint drew, and the lead it drew before it.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct PaintedBox {
    /// In window coordinates.
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) checked: bool,
    /// The lead's text and window `x`.
    pub(crate) lead: Option<(String, Pixels)>,
}

/// The lines and bars the last paint on this thread walked.
#[cfg(test)]
pub(crate) fn painted() -> (Range<usize>, Range<usize>) {
    PAINTED.with(|painted| painted.borrow().clone())
}

/// The code block backgrounds the last paint on this thread walked.
#[cfg(test)]
pub(crate) fn painted_blocks() -> Range<usize> {
    PAINTED_BLOCKS.with(|painted| painted.borrow().clone())
}

/// The window y of each line the last paint on this thread drew.
#[cfg(test)]
pub(crate) fn painted_tops() -> Vec<Pixels> {
    PAINTED_TOPS.with(|painted| painted.borrow().clone())
}

/// Each clipped line the last paint on this thread drew: its text's window x, and its mask.
#[cfg(test)]
pub(crate) fn painted_clips() -> Vec<(Pixels, Bounds<Pixels>)> {
    PAINTED_CLIPS.with(|painted| painted.borrow().clone())
}

/// Each task box the last paint on this thread drew, and the lead before it.
#[cfg(test)]
pub(crate) fn painted_boxes() -> Vec<PaintedBox> {
    PAINTED_BOXES.with(|painted| painted.borrow().clone())
}

impl DocumentLayout {
    /// Every code block background in the content mask; runs before the selection, which paints over it.
    pub(crate) fn paint_backgrounds(&self, origin: Point<Pixels>, window: &mut Window) {
        let mask = window.content_mask().bounds;
        let blocks = self.blocks_in(mask.top() - origin.y, mask.bottom() - origin.y);
        #[cfg(test)]
        PAINTED_BLOCKS.with(|painted| *painted.borrow_mut() = blocks.clone());
        for block in &self.code_blocks[blocks] {
            window.paint_quad(
                fill(
                    block.background + origin,
                    self.theme.colors.md_code_block_background,
                )
                .corner_radii(theme::CODE_BLOCK_RADIUS),
            );
        }
    }

    /// One `WrappedLine::paint` per line in the content mask; gpui steps rows down at each wrap. A
    /// failed line is skipped and logged, not fatal, so one bad glyph run doesn't blank the page.
    pub(crate) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let mask = window.content_mask().bounds;
        let lines = self.visible_lines(mask.top() - origin.y, mask.bottom() - origin.y);
        let bars = self.bars_of(&lines);
        #[cfg(test)]
        PAINTED.with(|painted| *painted.borrow_mut() = (lines.clone(), bars.clone()));
        #[cfg(test)]
        PAINTED_CLIPS.with(|painted| painted.borrow_mut().clear());
        #[cfg(test)]
        PAINTED_TOPS.with(|painted| painted.borrow_mut().clear());
        #[cfg(test)]
        PAINTED_BOXES.with(|painted| painted.borrow_mut().clear());
        for bar in &self.bars[bars] {
            window.paint_quad(fill(*bar + origin, self.theme.colors.md_quote_bar));
        }
        for index in lines {
            let placed = &self.lines[index];
            let line = &*placed.line;
            let line_origin = origin + point(px(0.), placed.top);
            #[cfg(test)]
            PAINTED_TOPS.with(|painted| painted.borrow_mut().push(line_origin.y));
            let clip = placed
                .block
                .and_then(|block| Some((self.block_clip(block)?, self.block_scroll(block))));
            let (x, mask) = match clip {
                // Horizontal only: the mask keeps the current top and bottom.
                Some((clip, scroll)) => {
                    let mask = window.content_mask().bounds;
                    let left = clip.left.map_or(mask.left(), |left| origin.x + left);
                    let bounds = Bounds::from_corners(
                        point(left, mask.top()),
                        point(origin.x + clip.right, mask.bottom()),
                    );
                    (line.indent() - scroll, Some(ContentMask { bounds }))
                }
                None => (line.indent(), None),
            };
            #[cfg(test)]
            if let Some(mask) = &mask {
                PAINTED_CLIPS.with(|painted| {
                    painted.borrow_mut().push((
                        line_origin.x + x,
                        window.content_mask().bounds.intersect(&mask.bounds),
                    ));
                });
            }
            let painted = window.with_content_mask(mask, |window| {
                line.shape.wrapped.paint(
                    line_origin + point(x, px(0.)),
                    line.row_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
            });
            if let Err(err) = painted {
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
            self.paint_task(line_origin, line, window, cx);
        }
        let blocks = self.blocks_in(mask.top() - origin.y, mask.bottom() - origin.y);
        for block in &self.code_blocks[blocks] {
            if let Some((at, shaped)) = &block.label
                && let Err(err) = shaped.paint(
                    origin + *at,
                    self.body_row_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
            {
                log::warn!("failed to paint a code block label: {err}");
            }
        }
    }

    /// Where the concealed task on `line`, at `origin`, paints its box and its lead (D10).
    fn paint_task(
        &self,
        origin: Point<Pixels>,
        line: &LineLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((square, task)) = line.task_box() else {
            return;
        };
        let lead = line.prefix.as_ref().and_then(Prefix::lead);
        let lead = lead.map(|(lead, x)| (lead, origin + point(x, px(0.))));
        self.paint_task_box(
            square + origin,
            task.checked,
            lead,
            line.row_height,
            window,
            cx,
        );
    }

    /// The lead at its point, then the square `bounds`: an outline, or an accent fill with a check.
    fn paint_task_box(
        &self,
        bounds: Bounds<Pixels>,
        checked: bool,
        lead: Option<(&ShapedLine, Point<Pixels>)>,
        row_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some((lead, at)) = lead
            && let Err(err) = lead.paint(at, row_height, TextAlign::Left, None, window, cx)
        {
            log::warn!("failed to paint a task's lead: {err}");
        }
        #[cfg(test)]
        PAINTED_BOXES.with(|painted| {
            painted.borrow_mut().push(PaintedBox {
                bounds,
                checked,
                lead: lead.map(|(lead, at)| (lead.text.to_string(), at.x)),
            });
        });
        let colors = &self.theme.colors;
        if !checked {
            window.paint_quad(
                outline(bounds, colors.md_marker, BorderStyle::Solid)
                    .border_widths(theme::TASK_BOX_BORDER)
                    .corner_radii(theme::TASK_BOX_RADIUS),
            );
            return;
        }
        window.paint_quad(fill(bounds, colors.accent).corner_radii(theme::TASK_BOX_RADIUS));
        let check = Bounds::centered_at(
            bounds.center(),
            size(theme::TASK_CHECK_SIZE, theme::TASK_CHECK_SIZE),
        );
        if let Err(err) = window.paint_svg(
            check,
            IconName::Check.path().into(),
            None,
            TransformationMatrix::unit(),
            colors.editor_background.into(),
            cx,
        ) {
            log::warn!("failed to paint a task's check: {err}");
        }
    }

    /// The flip of the task whose box holds `local`, and its text start, the gutter's end. The task
    /// is the one this layout's parse holds and the flip a diff from the text it shows, so
    /// `apply_diff` refuses it once the buffer moved on: matching bytes would not prove the task is
    /// the same one (task boxes spec, D5, G).
    pub(crate) fn task_at(&self, local: Point<Pixels>) -> Option<(Diff, ByteOffset)> {
        // Clamped to the nearest line: below its first row, its square misses.
        let (placed, _) = self.locate(self.row_for_y(local.y));
        let (square, _) = placed.line.task_box()?;
        let gutter = placed.line.prefix.as_ref()?.range();
        let gutters = &self.markdown.gutters;
        let task = gutters
            .get(gutters.partition_point(|gutter| gutter.range.start < placed.start))
            .filter(|gutter| gutter.range.start == placed.start)?
            .task
            .as_ref()?;
        square
            .contains(&(local - point(px(0.), placed.top)))
            .then(|| {
                let flip = Diff::new(self.shaped_at.0, vec![task.flip()]);
                (flip, placed.start + gutter.end)
            })
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

    /// The blocks reaching `top..bottom` in layout space; `code_blocks` is sorted by top and never overlaps.
    pub(super) fn blocks_in(&self, top: Pixels, bottom: Pixels) -> Range<usize> {
        let start = self
            .code_blocks
            .partition_point(|block| block.background.bottom() <= top);
        let end = self
            .code_blocks
            .partition_point(|block| block.background.top() < bottom);
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
