//! Every pixel ↔ offset conversion, in rows: one `shape_text` call per line, so a
//! keystroke does not re-shape the whole document.

mod rows;
mod runs;
mod shape;

#[cfg(test)]
pub(crate) use shape::shape_count;
pub(crate) use shape::shape_document;

use std::ops::{Range, RangeInclusive};

use gpui::{
    App, Bounds, PaintQuad, Pixels, Point, ShapedLine, TextAlign, TextStyle, Window, WrappedLine,
    fill, point, px, size,
};
use inkra_markdown::{DisplayLine, DisplayOffset, Markdown, touched};
use inkra_text::{Bias, Buffer, ByteOffset};

use crate::theme;

const NEWLINE_STUB: f32 = 0.25;
const CARET_WIDTH: Pixels = px(2.);

struct LineLayout {
    wrapped: WrappedLine,
    range: Range<ByteOffset>,
    display: DisplayLine,
    first_row: usize,
    rows: Vec<Row>,
    top: Pixels,
    row_height: Pixels,
    /// A heading's opening marker, kept out of the text in both states (live-preview spec, D12).
    hanging: Option<Hanging>,
}

struct Hanging {
    range: Range<ByteOffset>,
    /// `Some` only while revealed; painted left of the text and right-aligned to it.
    shaped: Option<ShapedLine>,
}

impl Hanging {
    /// Negative, since the text starts at 0; `None` while concealed.
    fn x_for_offset(&self, offset: ByteOffset) -> Option<Pixels> {
        let shaped = self.shaped.as_ref()?;
        Some(shaped.x_for_index(offset.0 - self.range.start.0) - shaped.width())
    }

    /// A concealed marker shows no glyph, so its whole margin lands on its start.
    fn offset_for_x(&self, x: Pixels) -> ByteOffset {
        let index = self
            .shaped
            .as_ref()
            .map_or(0, |shaped| shaped.closest_index_for_x(x + shaped.width()));
        ByteOffset(self.range.start.0 + index)
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

/// Every pixel↔offset conversion, for every `TextInput` host. Works in rows, not
/// lines, since a wrapped line paints on several.
pub(crate) struct DocumentLayout {
    lines: Vec<LineLayout>,
    row_count: usize,
    height: Pixels,
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
    pub(crate) fn is_shaped_from(
        &self,
        buffer: &Buffer,
        markdown: &Markdown,
        selection: &Range<ByteOffset>,
        marked: Option<&Range<ByteOffset>>,
        style: &TextStyle,
        wrap_width: Option<Pixels>,
    ) -> bool {
        self.version == buffer.version()
            && self.wrap_width == wrap_width
            && self.marked.as_ref() == marked
            && self.style == *style
            && touched(&markdown.conceals, &self.selection)
                .eq(touched(&markdown.conceals, selection))
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
        &self.lines[line].wrapped.text
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
            .iter()
            .position(|line| offset <= line.range.end)
            .unwrap_or(self.lines.len() - 1)
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

    /// Only row 0 holds the marker.
    fn hanging_on(line: &LineLayout, within: usize) -> Option<&Hanging> {
        line.hanging.as_ref().filter(|_| within == 0)
    }

    fn x_on_row(line: &LineLayout, within: usize, offset: ByteOffset) -> Pixels {
        let offset = offset.clamp(line.range.start, line.range.end);
        if let Some(x) = Self::hanging_on(line, within)
            .filter(|hanging| offset < hanging.range.end)
            .and_then(|hanging| hanging.x_for_offset(offset))
        {
            return x;
        }
        let index = line.display.to_display(offset).0;
        line.wrapped.unwrapped_layout.x_for_index(index) - line.rows[within].start_x
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
        // A hanging row splits at x = 0: the margin is the marker's, the text starts at its end (D12).
        let hanging = Self::hanging_on(line, within);
        if let Some(hanging) = hanging.filter(|_| x < px(0.)) {
            return hanging
                .offset_for_x(x)
                .clamp(target.start, target.caret_end);
        }
        let layout = &line.wrapped.unwrapped_layout;
        let index = layout.closest_index_for_x(x + target.start_x);
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
        hanging.map_or(offset, |hanging| offset.max(hanging.range.end))
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

    /// One `WrappedLine::paint` per line; gpui steps rows down at each wrap. A
    /// failed line is skipped and logged, not fatal, so one bad glyph run doesn't blank the page.
    pub(crate) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        for (index, line) in self.lines.iter().enumerate() {
            let line_origin = origin + point(px(0.), line.top);
            if let Err(err) = line.wrapped.paint(
                line_origin,
                line.row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("failed to paint line {index}: {err}");
            }
            if let Some(shaped) = line
                .hanging
                .as_ref()
                .and_then(|hanging| hanging.shaped.as_ref())
            {
                let marker_origin = line_origin - point(shaped.width(), px(0.));
                if let Err(err) = shaped.paint(
                    marker_origin,
                    line.row_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                ) {
                    eprintln!("failed to paint the marker of line {index}: {err}");
                }
            }
        }
    }
}
