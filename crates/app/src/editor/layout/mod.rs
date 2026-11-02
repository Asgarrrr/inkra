//! Every pixel ↔ offset conversion, in rows: one `shape_text` call per line, so a
//! keystroke does not re-shape the whole document.

mod paint;
mod prefix;
mod rows;
mod runs;
mod shape;

#[cfg(test)]
pub(crate) use paint::painted;
pub(crate) use shape::shape_document;
#[cfg(test)]
pub(crate) use shape::{line_shape_count, shape_count};

use std::ops::{Range, RangeInclusive};
use std::rc::Rc;

use gpui::{Bounds, PaintQuad, Pixels, Point, TextStyle, fill, point, px, size};
use inkra_markdown::{DisplayLine, DisplayOffset, Markdown, touched};
use inkra_text::{Bias, Buffer, ByteOffset};
use prefix::Prefix;
use shape::LineShape;

use crate::theme;

const NEWLINE_STUB: f32 = 0.25;
const CARET_WIDTH: Pixels = px(2.);

struct LineLayout {
    shape: Rc<LineShape>,
    range: Range<ByteOffset>,
    display: DisplayLine,
    first_row: usize,
    rows: Vec<Row>,
    top: Pixels,
    row_height: Pixels,
    /// Kept out of the shaped text in both states, so no reveal re-wraps the line.
    prefix: Option<Prefix>,
}

impl LineLayout {
    fn indent(&self) -> Pixels {
        prefix::indent(self.prefix.as_ref())
    }
}

/// A wrap offset belongs to the next row, never the earlier one's `caret_end`
/// (Zed's rule, `wrap_map.rs:969-981`) — this is what lets the caret skip affinity.
struct Row {
    /// On a grapheme boundary, even where gpui broke inside a cluster.
    start: ByteOffset,
    caret_end: ByteOffset,
    start_x: Pixels,
}

/// What a `DocumentLayout` is shaped from: `shape_document` takes it, `is_shaped_from` compares it.
pub(crate) struct LayoutInputs<'a> {
    pub(crate) buffer: &'a Buffer,
    pub(crate) markdown: &'a Markdown,
    pub(crate) selection: &'a Range<ByteOffset>,
    pub(crate) marked: Option<&'a Range<ByteOffset>>,
    pub(crate) style: &'a TextStyle,
    pub(crate) wrap_width: Option<Pixels>,
}

/// Every pixel↔offset conversion, for every `TextInput` host. Works in rows, not
/// lines, since a wrapped line paints on several.
pub(crate) struct DocumentLayout {
    lines: Vec<LineLayout>,
    row_count: usize,
    height: Pixels,
    /// One per quote level per line, from its top to the next line's (markdown blocks spec, D3),
    /// in line order, so sorted by top.
    bars: Vec<Bounds<Pixels>>,
    body_row_height: Pixels,
    /// What this was shaped with; outside a draw, `window.text_style()` is the default.
    style: TextStyle,
    wrap_width: Option<Pixels>,
    version: u64,
    selection: Range<ByteOffset>,
    marked: Option<Range<ByteOffset>>,
}

impl DocumentLayout {
    pub(crate) fn style(&self) -> &TextStyle {
        &self.style
    }

    pub(crate) fn wrap_width(&self) -> Option<Pixels> {
        self.wrap_width
    }

    /// Whether `shape_document` would give this layout back. The selection counts only
    /// through the elements it reveals, so a caret move within the same ones reuses it.
    pub(crate) fn is_shaped_from(&self, inputs: &LayoutInputs<'_>) -> bool {
        self.version == inputs.buffer.version()
            && self.wrap_width == inputs.wrap_width
            && self.marked.as_ref() == inputs.marked
            && self.style == *inputs.style
            && touched(&inputs.markdown.conceals, &self.selection)
                .eq(touched(&inputs.markdown.conceals, inputs.selection))
    }

    pub(crate) fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) fn row_height(&self, row: usize) -> Pixels {
        self.locate(row).0.row_height
    }

    pub(crate) fn body_row_height(&self) -> Pixels {
        self.body_row_height
    }

    /// The text line `line` was shaped from.
    #[cfg(test)]
    pub(crate) fn shown(&self, line: usize) -> &str {
        &self.lines[line].shape.wrapped.text
    }

    #[cfg(test)]
    pub(crate) fn quote_bars(&self) -> &[Bounds<Pixels>] {
        &self.bars
    }

    /// Whether line `line`'s prefix paints its source text.
    #[cfg(test)]
    pub(crate) fn prefix_painted(&self, line: usize) -> bool {
        self.lines[line]
            .prefix
            .as_ref()
            .and_then(Prefix::shown)
            .is_some()
    }

    pub(crate) fn height(&self) -> Pixels {
        self.height
    }

    /// `row_count` is one past the last row: its top is the document's bottom.
    pub(crate) fn top_for_row(&self, row: usize) -> Pixels {
        if row >= self.row_count {
            return self.height;
        }
        let (line, within) = self.locate(row);
        #[allow(clippy::cast_precision_loss)]
        let within = within as f32;
        line.top + line.row_height * within
    }

    /// Clamped: the layout can briefly describe other text than the buffer's
    /// (`NameField` shapes only in `prepaint`), so a stale caret beats an OOB index.
    fn line_for_offset(&self, offset: ByteOffset) -> usize {
        self.lines
            .partition_point(|line| line.range.end < offset)
            .min(self.lines.len() - 1)
    }

    pub(crate) fn row_for_offset(&self, offset: ByteOffset) -> usize {
        let line = &self.lines[self.line_for_offset(offset)];
        let within = line
            .rows
            .iter()
            .rposition(|row| row.start <= offset)
            .unwrap_or(0);
        line.first_row + within
    }

    fn locate(&self, row: usize) -> (&LineLayout, usize) {
        let row = row.min(self.row_count - 1);
        let line = self.lines.partition_point(|line| line.first_row <= row) - 1;
        let line = &self.lines[line];
        (line, row - line.first_row)
    }

    /// Clamped: mouse listeners sit on the view's full-size `div`, so a click
    /// below a short document arrives with `y` past the last row.
    fn row_for_y(&self, y: Pixels) -> usize {
        if y <= px(0.) {
            return 0;
        }
        // `lines[0].top` is zero and `y` is positive, so at least one line qualifies.
        let line = &self.lines[self.lines.partition_point(|line| line.top <= y) - 1];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let within = ((y - line.top) / line.row_height) as usize;
        line.first_row + within.min(line.rows.len() - 1)
    }

    /// Only row 0 holds the prefix.
    fn prefix_on(line: &LineLayout, within: usize) -> Option<&Prefix> {
        line.prefix.as_ref().filter(|_| within == 0)
    }

    fn x_on_row(line: &LineLayout, within: usize, offset: ByteOffset) -> Pixels {
        let offset = offset.clamp(line.range.start, line.range.end);
        if let Some(x) = Self::prefix_on(line, within)
            .filter(|prefix| offset < prefix.range().end)
            .and_then(|prefix| prefix.x_for_offset(offset))
        {
            return x;
        }
        let index = line.display.to_display(offset).0;
        line.indent() + line.shape.wrapped.unwrapped_layout.x_for_index(index)
            - line.rows[within].start_x
    }

    fn row_limit(line: &LineLayout, within: usize) -> ByteOffset {
        line.rows
            .get(within + 1)
            .map_or(line.range.end, |next| next.start)
    }

    fn row_end_x(line: &LineLayout, within: usize) -> Pixels {
        Self::x_on_row(line, within, Self::row_limit(line, within))
    }

    pub(crate) fn x_for_offset(&self, offset: ByteOffset) -> Pixels {
        let (line, within) = self.locate(self.row_for_offset(offset));
        Self::x_on_row(line, within, offset)
    }

    #[cfg(test)]
    pub(crate) fn position_for_offset(&self, offset: ByteOffset) -> Point<Pixels> {
        point(
            self.x_for_offset(offset),
            self.top_for_row(self.row_for_offset(offset)),
        )
    }

    /// The caret at `offset`: as tall as its row, which a heading makes taller than body text.
    pub(crate) fn caret_bounds(&self, offset: ByteOffset) -> Bounds<Pixels> {
        let row = self.row_for_offset(offset);
        Bounds::new(
            point(self.x_for_offset(offset), self.top_for_row(row)),
            size(CARET_WIDTH, self.row_height(row)),
        )
    }

    pub(crate) fn offset_for_position(&self, position: Point<Pixels>) -> ByteOffset {
        self.offset_on_row(self.row_for_y(position.y), position.x)
    }

    /// Clamped to the row's own range, so landing past its end stops on
    /// `caret_end` rather than the next row's start.
    pub(crate) fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset {
        let (line, within) = self.locate(row);
        let target = &line.rows[within];
        // Row 0 splits at the text's start: left of it is the prefix's, and the text never maps into the prefix (D12).
        let prefix = Self::prefix_on(line, within);
        if let Some(prefix) = prefix.filter(|prefix| x < prefix.indent()) {
            return prefix.offset_for_x(x).clamp(target.start, target.caret_end);
        }
        let layout = &line.shape.wrapped.unwrapped_layout;
        let index = layout.closest_index_for_x(x - line.indent() + target.start_x);
        // At or past the display end, land after a hidden suffix (live-preview spec, D5).
        let bias = if index >= layout.len {
            Bias::Right
        } else {
            Bias::Left
        };
        let offset = line
            .display
            .to_buffer(DisplayOffset(index), bias)
            .clamp(target.start, target.caret_end);
        prefix.map_or(offset, |prefix| offset.max(prefix.range().end))
    }

    pub(crate) fn row_bounds(&self, row: usize) -> RangeInclusive<ByteOffset> {
        let (line, within) = self.locate(row);
        let row = &line.rows[within];
        row.start..=row.caret_end
    }

    /// False when this row's only position also belongs to the next: two wraps
    /// in one cluster share a start, so vertical moves must step over it.
    pub(crate) fn holds_caret(&self, row: usize) -> bool {
        self.row_for_offset(*self.row_bounds(row).start()) == row
    }

    pub(crate) fn range_bounds(&self, range: &Range<ByteOffset>) -> Bounds<Pixels> {
        let row = self.row_for_offset(range.start);
        let (line, within) = self.locate(row);
        // max(range.start): a stale layout can put range.start past the
        // clamped row's end, which would otherwise return an inverted rect.
        let end = range
            .end
            .min(Self::row_limit(line, within))
            .max(range.start);
        let top = self.top_for_row(row);
        Bounds::from_corners(
            point(Self::x_on_row(line, within, range.start), top),
            point(Self::x_on_row(line, within, end), top + line.row_height),
        )
    }

    /// One quad per row touched. Four cases: a one-row selection is both first
    /// and last, and a row ending in a wrap gets no [`NEWLINE_STUB`] gap since a wrap isn't a character.
    pub(crate) fn selection_quads(&self, selection: &Range<ByteOffset>) -> Vec<PaintQuad> {
        let first = self.row_for_offset(selection.start);
        let last = self.row_for_offset(selection.end);

        (first..=last)
            .map(|row| {
                let (line, within) = self.locate(row);
                let left = if row == first {
                    Self::x_on_row(line, within, selection.start)
                } else {
                    Self::x_on_row(line, within, line.rows[within].start)
                };
                let right = if row == last {
                    Self::x_on_row(line, within, selection.end)
                } else if within + 1 == line.rows.len() {
                    Self::row_end_x(line, within) + line.row_height * NEWLINE_STUB
                } else {
                    Self::row_end_x(line, within)
                };
                let top = self.top_for_row(row);
                fill(
                    Bounds::from_corners(point(left, top), point(right, top + line.row_height)),
                    theme::selection(),
                )
            })
            .collect()
    }
}
