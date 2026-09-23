//! Layout and painting: one `shape_text` call per line, so a keystroke does not
//! re-shape the whole document (the layout cache keys on the full text otherwise).

use std::cell::RefCell;
use std::ops::{Range, RangeInclusive};
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId,
    HighlightStyle, LayoutId, PaintQuad, Pixels, Point, SharedString, Style, TextAlign, TextRun,
    TextStyle, UnderlineStyle, Window, WrappedLine, fill, point, prelude::*, px, relative, size,
};
use inkra_text::{Buffer, ByteOffset};
use unicode_segmentation::GraphemeCursor;

use super::EditorView;
use crate::theme;

const NEWLINE_STUB: f32 = 0.25;

struct LineLayout {
    wrapped: WrappedLine,
    range: Range<ByteOffset>,
    first_row: usize,
    rows: Vec<Row>,
}

/// A wrap offset belongs to the next row, never the earlier one's `caret_end`
/// (Zed's rule, `wrap_map.rs:969-981`) — this is what lets the caret skip affinity.
struct Row {
    /// On a grapheme boundary, even where gpui broke inside a cluster.
    start: ByteOffset,
    caret_end: ByteOffset,
    start_x: Pixels,
}

/// Every pixel↔offset conversion, shared by the editor and `NameField`'s
/// [`shape_document`]. Works in rows, not lines, since a wrapped line paints on several.
pub(crate) struct DocumentLayout {
    lines: Vec<LineLayout>,
    row_count: usize,
    line_height: Pixels,
    /// What this was shaped with; outside a draw, `window.text_style()` is the default.
    style: TextStyle,
    wrap_width: Option<Pixels>,
}

impl DocumentLayout {
    pub(crate) fn style(&self) -> &TextStyle {
        &self.style
    }

    pub(crate) fn wrap_width(&self) -> Option<Pixels> {
        self.wrap_width
    }

    pub(crate) fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) fn line_height(&self) -> Pixels {
        self.line_height
    }

    pub(crate) fn height(&self) -> Pixels {
        self.top_for_row(self.row_count)
    }

    pub(crate) fn top_for_row(&self, row: usize) -> Pixels {
        #[allow(clippy::cast_precision_loss)]
        let row = row as f32;
        self.line_height * row
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
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (y / self.line_height) as usize;
        row.min(self.row_count - 1)
    }

    fn x_on_row(line: &LineLayout, within: usize, offset: ByteOffset) -> Pixels {
        let index = offset.0.clamp(line.range.start.0, line.range.end.0) - line.range.start.0;
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

    pub(crate) fn position_for_offset(&self, offset: ByteOffset) -> Point<Pixels> {
        point(
            self.x_for_offset(offset),
            self.top_for_row(self.row_for_offset(offset)),
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
        let index = line
            .wrapped
            .unwrapped_layout
            .closest_index_for_x(x + target.start_x);
        ByteOffset(line.range.start.0 + index).clamp(target.start, target.caret_end)
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

    pub(super) fn range_bounds(&self, range: &Range<ByteOffset>) -> Bounds<Pixels> {
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
            point(Self::x_on_row(line, within, end), top + self.line_height),
        )
    }

    /// One quad per row touched. Four cases: a one-row selection is both first
    /// and last, and a row ending in a wrap gets no [`NEWLINE_STUB`] gap since a wrap isn't a character.
    pub(super) fn selection_quads(&self, selection: &Range<ByteOffset>) -> Vec<PaintQuad> {
        let first = self.row_for_offset(selection.start);
        let last = self.row_for_offset(selection.end);

        (first..=last)
            .map(|row| {
                let (line, within) = self.locate(row);
                let left = if row == first {
                    Self::x_on_row(line, within, selection.start)
                } else {
                    px(0.)
                };
                let right = if row == last {
                    Self::x_on_row(line, within, selection.end)
                } else if within + 1 == line.rows.len() {
                    Self::row_end_x(line, within) + self.line_height * NEWLINE_STUB
                } else {
                    Self::row_end_x(line, within)
                };
                let top = self.top_for_row(row);
                fill(
                    Bounds::from_corners(point(left, top), point(right, top + self.line_height)),
                    theme::selection(),
                )
            })
            .collect()
    }

    /// One `WrappedLine::paint` per line; gpui steps rows down at each wrap. A
    /// failed line is skipped and logged, not fatal, so one bad glyph run doesn't blank the page.
    pub(crate) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        for (index, line) in self.lines.iter().enumerate() {
            let line_origin = origin + point(px(0.), self.top_for_row(line.first_row));
            if let Err(err) = line.wrapped.paint(
                line_origin,
                self.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("failed to paint line {index}: {err}");
            }
        }
    }
}

pub(super) struct TextElement {
    pub(super) editor: Entity<EditorView>,
}

pub(super) struct PrepaintState {
    layout: Option<DocumentLayout>,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

/// Runs indexed from zero within the line: the marked range is clipped and
/// rebased, since left absolute it would stop summing to the line's length after the first line.
fn line_runs(
    style: &TextStyle,
    composing: &TextStyle,
    line: &Range<ByteOffset>,
    marked: Option<&Range<ByteOffset>>,
) -> Vec<TextRun> {
    let length = line.end.0 - line.start.0;
    let marked = marked
        .map(|marked| marked.start.max(line.start)..marked.end.min(line.end))
        .filter(|marked| marked.start < marked.end);

    let Some(marked) = marked else {
        return vec![style.to_run(length)];
    };
    let lead = marked.start.0 - line.start.0;
    let composed = marked.end.0 - marked.start.0;
    [
        style.to_run(lead),
        composing.to_run(composed),
        style.to_run(length - lead - composed),
    ]
    .into_iter()
    .filter(|run| run.len > 0)
    .collect()
}

/// Takes `style` explicitly since the editor also shapes outside any draw,
/// where `window.text_style()` doesn't apply. `wrap_width: None` never wraps.
pub(crate) fn shape_document(
    buffer: &Buffer,
    marked: Option<&Range<ByteOffset>>,
    style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> DocumentLayout {
    let composing = style.clone().highlight(HighlightStyle {
        underline: Some(UnderlineStyle {
            color: Some(style.color),
            thickness: px(1.0),
            wavy: false,
        }),
        ..HighlightStyle::default()
    });
    let font_size = style.font_size.to_pixels(window.rem_size());

    let mut row_count = 0;
    let lines = buffer
        .lines()
        .map(|(start, text)| {
            let range = start..ByteOffset(start.0 + text.len());
            let runs = line_runs(style, &composing, &range, marked);
            let wrapped = window
                .text_system()
                .shape_text(
                    SharedString::from(text.to_string()),
                    font_size,
                    &runs,
                    wrap_width,
                    None,
                )
                .map_err(|err| eprintln!("failed to shape line at {start}: {err}"))
                .ok()
                .and_then(|mut shaped| shaped.pop())
                .unwrap_or_default();
            let rows = rows(text, &wrapped, &range);
            let first_row = row_count;
            row_count += rows.len();
            LineLayout {
                wrapped,
                range,
                first_row,
                rows,
            }
        })
        .collect();

    DocumentLayout {
        lines,
        row_count,
        line_height: style.line_height_in_pixels(window.rem_size()),
        style: style.clone(),
        wrap_width,
    }
}

/// One row per gpui row. Each wrap point snaps forward to the next grapheme
/// boundary, so no caret lands inside a cluster (the glyphs before the snap still paint on the new row).
fn rows(text: &str, wrapped: &WrappedLine, range: &Range<ByteOffset>) -> Vec<Row> {
    let layout = &wrapped.unwrapped_layout;
    let at = |index: usize| ByteOffset(range.start.0 + index);

    let mut starts = vec![(0, px(0.))];
    for boundary in wrapped.wrap_boundaries() {
        let glyph = &layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix];
        let snapped = grapheme_at_or_after(text, glyph.index);
        // Monotonic: two wraps snapping to one cluster end must not collide.
        let previous = starts.last().map_or(0, |(start, _)| *start);
        starts.push((snapped.max(previous), glyph.position.x));
    }

    let ends = starts
        .iter()
        .skip(1)
        .map(|(next, _)| Some(*next))
        .chain([None]);
    starts
        .iter()
        .zip(ends)
        .map(|(&(start, start_x), next)| Row {
            start: at(start),
            caret_end: at(next.map_or(text.len(), |next| grapheme_before(text, next).max(start))),
            start_x,
        })
        .collect()
}

fn grapheme_at_or_after(text: &str, index: usize) -> usize {
    let mut cursor = GraphemeCursor::new(index, text.len(), true);
    if cursor.is_boundary(text, 0).unwrap_or(true) {
        return index;
    }
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

fn grapheme_before(text: &str, index: usize) -> usize {
    GraphemeCursor::new(index, text.len(), true)
        .prev_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(0)
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Every layout shaped this frame, keyed by width: taffy may call the measure
/// closure several times and answer its final pass from a cache, so the last call isn't necessarily final.
type Stash = Rc<RefCell<Vec<DocumentLayout>>>;

impl Element for TextElement {
    type RequestLayoutState = Stash;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Read here: the closure runs in `compute_layout`, outside the div's
        // text style, where `window.text_style()` would be the default instead.
        let text_style = window.text_style();
        let column = column_width(&text_style, window);

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.max_size.width = column.into();
        // Flex column's default flex_shrink of 1 would squeeze an overtall
        // document back down instead of overflowing into the scroll container.
        style.flex_shrink = 0.;

        let stash = Stash::default();
        let editor = self.editor.clone();
        let measured = stash.clone();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, cx| {
                // The sizing pass hands `known` unclamped by max_size; only the
                // layout pass clamps, so the column is reapplied here.
                let width = known
                    .width
                    .or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                    .map_or(column, |width| width.min(column));

                // Wrapped text has no minimum width; reporting the column here
                // would become the flex item's automatic minimum and push it past a narrower pane.
                let reported = match (known.width, available.width) {
                    (None, AvailableSpace::MinContent) => px(0.),
                    _ => width,
                };

                let mut stash = measured.borrow_mut();
                if let Some(layout) = stash.iter().find(|layout| layout.wrap_width == Some(width)) {
                    return size(reported, layout.height());
                }
                let editor = editor.read(cx);
                let marked = editor.state.marked_range();
                let layout = shape_document(
                    editor.buffer.read(cx),
                    marked.as_ref(),
                    &text_style,
                    Some(width),
                    window,
                );
                let height = layout.height();
                stash.push(layout);
                size(reported, height)
            });
        (layout_id, stash)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        stash: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let layout = take_measured(stash, bounds.size.width).unwrap_or_else(|| {
            // One frame stale height; corrects next frame. gpui's own `Text`
            // panics here (`elements/text.rs:782-790`) — a note editor must not.
            eprintln!("no layout measured at width {}", bounds.size.width);
            let editor = self.editor.read(cx);
            let marked = editor.state.marked_range();
            let style = window.text_style();
            shape_document(
                editor.buffer.read(cx),
                marked.as_ref(),
                &style,
                Some(bounds.size.width),
                window,
            )
        });

        let editor = self.editor.read(cx);
        let selection = editor.state.selection().range();
        let cursor = editor.cursor();
        let (selection, cursor) = if selection.is_empty() {
            let at = layout.position_for_offset(cursor);
            let quad = fill(
                Bounds::new(bounds.origin + at, size(px(2.), layout.line_height)),
                theme::cursor(),
            );
            (Vec::new(), Some(quad))
        } else {
            let quads = layout
                .selection_quads(&selection)
                .into_iter()
                .map(|quad| translate(quad, bounds.origin))
                .collect();
            (quads, None)
        };

        PrepaintState {
            layout: Some(layout),
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        let Some(layout) = prepaint.layout.take() else {
            return;
        };
        layout.paint(bounds.origin, window, cx);

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.editor.update(cx, |editor, _| {
            editor.last_layout = Some(layout);
            editor.last_bounds = Some(bounds);
            // Read now: the scroll offset here is what translated this frame's
            // `bounds` (`elements/div.rs:2390`), so subtracting it gives the y at zero scroll.
            editor.last_text_top = Some(bounds.origin.y - editor.scroll_handle.offset().y);
        });
    }
}

/// The measured layout nearest the element's width — not equal, since bounds
/// are pixel-snapped after taffy measured them.
fn take_measured(stash: &Stash, width: Pixels) -> Option<DocumentLayout> {
    let mut stash = stash.borrow_mut();
    let distance = |layout: &DocumentLayout| {
        layout.wrap_width.map_or(f32::INFINITY, |measured| {
            f32::from((measured - width).abs())
        })
    };
    let index =
        (0..stash.len()).min_by(|&a, &b| distance(&stash[a]).total_cmp(&distance(&stash[b])))?;
    Some(stash.swap_remove(index))
}

pub(super) fn column_width(style: &TextStyle, window: &mut Window) -> Pixels {
    let font_size = style.font_size.to_pixels(window.rem_size());
    let font = window.text_system().resolve_font(&style.font());
    let ch = window
        .text_system()
        .ch_advance(font, font_size)
        .unwrap_or(font_size / 2.);
    ch * theme::BODY_COLUMN_CHARS
}

fn translate(mut quad: PaintQuad, origin: Point<Pixels>) -> PaintQuad {
    quad.bounds.origin += origin;
    quad
}
