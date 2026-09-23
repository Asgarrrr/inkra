//! Layout and painting.
//!
//! One `shape_text` call per logical line, wrapped at the column: shaping the
//! document as one string would key the frame-to-frame layout cache on the
//! whole text, so one keystroke would miss for every line
//! (`gpui/text_system/line_layout.rs:601-665`).

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

/// Painted past the end of a line that is selected through its line break, so a
/// selected newline is visible rather than stopping flush against the last
/// glyph. A fraction of the line height, because there is no glyph to measure.
const NEWLINE_STUB: f32 = 0.25;

struct LineLayout {
    wrapped: WrappedLine,
    /// Excludes the terminating newline, matching [`Buffer::line_range`].
    range: Range<ByteOffset>,
    first_row: usize,
    /// One per visual row, in order; never empty.
    rows: Vec<Row>,
}

/// One visual row of a wrapped line.
///
/// An offset at a wrap is both the end of one row and the start of the next.
/// It belongs to the next: `row_for_offset` says so, and nothing that targets
/// the earlier row can produce it, because every landing there clamps to
/// `caret_end`. That is Zed's rule (`editor/src/display_map/wrap_map.rs:969-981`),
/// and it is what lets the caret stay a bare `ByteOffset` with no affinity.
struct Row {
    /// On a grapheme boundary, even where gpui broke inside a cluster.
    start: ByteOffset,
    /// Where End and a click past the row land: one grapheme before the next
    /// row's start, or the line's end on its last row.
    caret_end: ByteOffset,
    /// gpui's x for the glyph it broke at, which is where it starts painting
    /// the row (`gpui/src/text_system/line.rs:521`).
    start_x: Pixels,
}

/// Every pixel↔offset conversion, in one place — shared by the editor and
/// `NameField`, which shapes its one-line buffer through the same
/// [`shape_document`] rather than a second shaper of its own.
///
/// Rows, not lines: a line longer than the wrap width paints on several, and
/// every caller — the cursor quad, vertical movement, hit-testing, the input
/// method — asks about the row. Coordinates are relative to the element's
/// origin; the caller adds it.
pub(crate) struct DocumentLayout {
    /// Never empty: an empty buffer is one empty line.
    lines: Vec<LineLayout>,
    row_count: usize,
    line_height: Pixels,
    /// What this was shaped with, so a gate can shape the next text the same
    /// way: outside a draw, `window.text_style()` is the default, not ours
    /// (`gpui/src/window.rs:2363-2369`).
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

    /// Clamped, and deliberately so. The layout can describe other text than
    /// the buffer's for a moment — `NameField` shapes in `prepaint` only — so
    /// the buffer can hold offsets this has no row for. Clamping keeps that a
    /// caret one frame out of date instead of an index out of bounds.
    fn line_for_offset(&self, offset: ByteOffset) -> usize {
        self.lines
            .iter()
            .position(|line| offset <= line.range.end)
            .unwrap_or(self.lines.len() - 1)
    }

    /// The last row of the offset's line that starts at or before it — so an
    /// offset at a wrap lands on the later row.
    pub(crate) fn row_for_offset(&self, offset: ByteOffset) -> usize {
        let line = &self.lines[self.line_for_offset(offset)];
        let within = line
            .rows
            .iter()
            .rposition(|row| row.start <= offset)
            .unwrap_or(0);
        line.first_row + within
    }

    /// The line holding `row` and the row's index within it. Clamped like
    /// [`Self::line_for_offset`].
    fn locate(&self, row: usize) -> (&LineLayout, usize) {
        let row = row.min(self.row_count - 1);
        let line = self.lines.partition_point(|line| line.first_row <= row) - 1;
        let line = &self.lines[line];
        (line, row - line.first_row)
    }

    /// Clamped for the same reason, plus one of its own: the mouse listeners sit
    /// on the view's full-size `div`, not on this element, so a click in the
    /// empty space below a short document arrives with a `y` past the last row.
    fn row_for_y(&self, y: Pixels) -> usize {
        if y <= px(0.) {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (y / self.line_height) as usize;
        row.min(self.row_count - 1)
    }

    /// `offset`'s x measured from the start of row `within` of `line`, whether
    /// or not the offset lies on that row — what a quad's right edge needs when
    /// its end is the next row's start.
    fn x_on_row(line: &LineLayout, within: usize, offset: ByteOffset) -> Pixels {
        let index = offset.0.clamp(line.range.start.0, line.range.end.0) - line.range.start.0;
        line.wrapped.unwrapped_layout.x_for_index(index) - line.rows[within].start_x
    }

    /// Where row `within` of `line` stops: the next row's start, or the line's
    /// end. One past `caret_end`, and not a caret position on this row.
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

    /// Unclipped in the byte sense: every caller hands it to `EditorState`,
    /// which clips it against the live buffer.
    pub(crate) fn offset_for_position(&self, position: Point<Pixels>) -> ByteOffset {
        self.offset_on_row(self.row_for_y(position.y), position.x)
    }

    /// Clamped to the row's own range, so a landing past its end stops on
    /// `caret_end` rather than on the next row's start.
    pub(crate) fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset {
        let (line, within) = self.locate(row);
        let target = &line.rows[within];
        let index = line
            .wrapped
            .unwrapped_layout
            .closest_index_for_x(x + target.start_x);
        ByteOffset(line.range.start.0 + index).clamp(target.start, target.caret_end)
    }

    /// The first and last caret positions on `row`: where Home and End land.
    pub(crate) fn row_bounds(&self, row: usize) -> RangeInclusive<ByteOffset> {
        let (line, within) = self.locate(row);
        let row = &line.rows[within];
        row.start..=row.caret_end
    }

    /// False for a row whose only position belongs to the next one: two wraps
    /// inside one cluster snap to the same start. Vertical moves step over it,
    /// or they would land back on the row they left.
    pub(crate) fn holds_caret(&self, row: usize) -> bool {
        self.row_for_offset(*self.row_bounds(row).start()) == row
    }

    /// The rect macOS anchors the input method's candidate window to.
    ///
    /// Clamped to the row holding `range.start`: a rect spanning three rows
    /// anchors the window nowhere useful.
    pub(super) fn range_bounds(&self, range: &Range<ByteOffset>) -> Bounds<Pixels> {
        let row = self.row_for_offset(range.start);
        let (line, within) = self.locate(row);
        // `max(range.start)` because a stale layout can put `range.start` past
        // the clamped row's end, which would otherwise return an inverted rect.
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

    /// One quad per row the selection touches.
    ///
    /// Four cases, not three: a selection contained in one row is both the
    /// first and the last, and must not pick up the first row's run-to-the-end
    /// rule. A row that ends its line inside the selection gets [`NEWLINE_STUB`]
    /// past its last glyph so the selected line break shows; a row that ends in
    /// a wrap does not, because a wrap is not a character.
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

    /// One `WrappedLine::paint` call per line, at `origin` translated down to
    /// the line's first row; gpui steps down a row at each wrap itself. Shared
    /// by `TextElement` and `FieldElement` so the two painters cannot drift on
    /// how a line is placed.
    ///
    /// A failed line is reported and skipped rather than aborting the rest of
    /// the document — one glyph run the platform shaper rejects should not
    /// blank the page.
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

/// The runs for one line, indexed from zero *within that line*.
///
/// The marked range is clipped to the line and rebased. Left absolute, the run
/// lengths stop summing to the line's length on every line after the first, and
/// `layout_line` hands those lengths straight to the platform shaper
/// (`gpui/text_system.rs:886`) — mis-shaped text, not a panic.
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

/// Takes the style rather than reading `window.text_style()`, because the
/// editor also shapes from an action handler, outside any draw. `wrap_width`
/// `None` never wraps: one row per line.
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
                // One line in, one line out: `text` holds no newline.
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

/// One row per gpui row — never fewer, or the painted rows would not line up
/// with the height reserved for them.
///
/// gpui breaks at a glyph, which can fall inside a grapheme cluster. Each start
/// is snapped forward to the cluster's end, so no caret lands between a letter
/// and its accent; the glyphs before the snap still paint on the new row, which
/// is gpui's call, not ours.
///
/// Segmented over the line's own `text`, not through `Buffer`'s grapheme steps:
/// those scan the whole document per call, and this runs once per wrap per
/// frame.
fn rows(text: &str, wrapped: &WrappedLine, range: &Range<ByteOffset>) -> Vec<Row> {
    let layout = &wrapped.unwrapped_layout;
    let at = |index: usize| ByteOffset(range.start.0 + index);

    let mut starts = vec![(0, px(0.))];
    for boundary in wrapped.wrap_boundaries() {
        let glyph = &layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix];
        let snapped = grapheme_at_or_after(text, glyph.index);
        // Monotonic even if two breaks snap to one cluster end: the second row
        // then has a single caret position.
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

/// Every layout the measure closure shaped this frame, one per width.
///
/// Keyed by width rather than holding the last one: taffy may call the closure
/// several times per frame and answer its final pass from a cache
/// (`taffy-0.13.0/src/compute/leaf.rs:111-141`), so the last call is not
/// necessarily the width it settled on.
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

    /// Measured, not sized: the height is the row count, and the row count
    /// depends on the width taffy gives us. gpui's `Text` does the same
    /// (`gpui/src/elements/text.rs:647`).
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Read here: the closure runs in `compute_layout`, outside the `div`'s
        // text style, where `window.text_style()` is the default.
        let text_style = window.text_style();
        let column = column_width(&text_style, window);

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.max_size.width = column.into();
        // The parent is a flex column, whose default `flex_shrink` of 1 would
        // squeeze a document taller than the window back down to it — silently
        // defeating the scroll container instead of overflowing into it.
        style.flex_shrink = 0.;

        let stash = Stash::default();
        let editor = self.editor.clone();
        let measured = stash.clone();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, cx| {
                // The sizing pass hands `known` unclamped by `max_size`; only
                // the layout pass clamps. So the column is applied here too.
                let width = known
                    .width
                    .or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                    .map_or(column, |width| width.min(column));

                // Wrapped text breaks mid-word when it must, so it has no
                // minimum width. Reporting the column here would become the
                // flex item's automatic minimum and push it past a narrower pane.
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
            // Rows right, reserved height one frame stale: the next frame's
            // measure corrects it. gpui's `Text` panics here
            // (`gpui/src/elements/text.rs:782-790`); a note editor must not.
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
            // Read here rather than recomputed later: the offset in the handle
            // right now is the one this frame's `bounds` was translated by
            // (`gpui/elements/div.rs:2390-2392`, then `:2027`), so subtracting
            // it leaves the y the text would have at zero scroll.
            editor.last_text_top = Some(bounds.origin.y - editor.scroll_handle.offset().y);
        });
    }
}

/// The measured layout taffy settled on: the one nearest the element's width.
/// Nearest rather than equal, because bounds are pixel-snapped after taffy
/// measured them.
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

/// [`theme::BODY_COLUMN_CHARS`] in pixels: the CSS `ch`, the advance of `0`.
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
