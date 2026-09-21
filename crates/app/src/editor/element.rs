//! Layout and painting.
//!
//! One `shape_line` call per logical line: it asserts its text holds no newline
//! (`gpui/text_system.rs:645`), and shaping the document as one string would key the
//! frame-to-frame layout cache on the whole text, so one keystroke would miss
//! for every line (`gpui/text_system/line_layout.rs:936-942`).

use std::ops::Range;

use gpui::{
    App, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, HighlightStyle, LayoutId,
    PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextAlign, TextRun, TextStyle,
    UnderlineStyle, Window, fill, point, prelude::*, px, relative, size,
};
use inkra_text::{Buffer, ByteOffset};

use super::EditorView;
use crate::theme;

/// Painted past the end of a line that is selected through its line break, so a
/// selected newline is visible rather than stopping flush against the last
/// glyph. A fraction of the line height, because there is no glyph to measure.
const NEWLINE_STUB: f32 = 0.25;

struct LineLayout {
    shaped: ShapedLine,
    /// Excludes the terminating newline, matching [`Buffer::line_range`].
    range: Range<ByteOffset>,
}

/// Every pixel↔offset conversion in the editor, in one place.
///
/// Six call sites need it — the cursor quad, the selection quads,
/// `bounds_for_range`, `character_index_for_point`, mouse hit-testing and
/// vertical movement. Resolving a line four different ways is how they drift.
///
/// All coordinates are relative to the element's origin. The caller adds it.
pub(super) struct DocumentLayout {
    /// Never empty: an empty buffer is one empty line.
    lines: Vec<LineLayout>,
    line_height: Pixels,
}

impl DocumentLayout {
    pub(super) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn line_height(&self) -> Pixels {
        self.line_height
    }

    pub(super) fn top_for_line(&self, line: usize) -> Pixels {
        #[allow(clippy::cast_precision_loss)]
        let line = line as f32;
        self.line_height * line
    }

    /// Clamped, and deliberately so. This layout describes the text as of the
    /// last paint, so after an edit that added or removed a line the buffer has
    /// offsets this has no row for. Clamping keeps that a caret one frame out
    /// of date — the staleness the field already documents — instead of an
    /// index out of bounds.
    pub(super) fn line_for_offset(&self, offset: ByteOffset) -> usize {
        self.lines
            .iter()
            .position(|line| offset <= line.range.end)
            .unwrap_or(self.lines.len() - 1)
    }

    /// Clamped for the same reason, plus one of its own: the mouse listeners sit
    /// on the view's full-size `div`, not on this element, so a click in the
    /// empty space below a short document arrives with a `y` past the last row.
    fn line_for_y(&self, y: Pixels) -> usize {
        if y <= px(0.) {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let line = (y / self.line_height) as usize;
        line.min(self.lines.len() - 1)
    }

    pub(super) fn x_for_offset(&self, offset: ByteOffset) -> Pixels {
        let line = &self.lines[self.line_for_offset(offset)];
        let index = offset.0.clamp(line.range.start.0, line.range.end.0) - line.range.start.0;
        line.shaped.x_for_index(index)
    }

    pub(super) fn position_for_offset(&self, offset: ByteOffset) -> Point<Pixels> {
        point(
            self.x_for_offset(offset),
            self.top_for_line(self.line_for_offset(offset)),
        )
    }

    /// Unclipped in the byte sense: the result addresses the text as of the last
    /// paint. Every caller hands it to `EditorState`, which clips it against the
    /// live buffer.
    pub(super) fn offset_for_position(&self, position: Point<Pixels>) -> ByteOffset {
        self.offset_for_x_on_line(self.line_for_y(position.y), position.x)
    }

    pub(super) fn offset_for_x_on_line(&self, line: usize, x: Pixels) -> ByteOffset {
        let line = &self.lines[line.min(self.lines.len() - 1)];
        ByteOffset(line.range.start.0 + line.shaped.closest_index_for_x(x))
    }

    /// The rect macOS anchors the input method's candidate window to.
    ///
    /// Clamped to the line holding `range.start`: a rect spanning three lines
    /// anchors the window nowhere useful.
    pub(super) fn range_bounds(&self, range: &Range<ByteOffset>) -> Bounds<Pixels> {
        let line = self.line_for_offset(range.start);
        // `max(range.start)` because a stale layout can put `range.start` past
        // the clamped line's end, which would otherwise return an inverted rect.
        let end = range.end.min(self.lines[line].range.end).max(range.start);
        let top = self.top_for_line(line);
        Bounds::from_corners(
            point(self.x_for_offset(range.start), top),
            point(self.x_for_offset(end), top + self.line_height),
        )
    }

    /// One quad per line the selection touches.
    ///
    /// Four cases, not three: a selection contained in one line is both the
    /// first and the last, and must not pick up the first line's run-to-the-end
    /// rule. Lines that end inside the selection get [`NEWLINE_STUB`] past their
    /// last glyph so the selected line break shows.
    fn selection_quads(&self, selection: &Range<ByteOffset>) -> Vec<PaintQuad> {
        let first = self.line_for_offset(selection.start);
        let last = self.line_for_offset(selection.end);

        (first..=last)
            .map(|line| {
                let left = if line == first {
                    self.x_for_offset(selection.start)
                } else {
                    px(0.)
                };
                let right = if line == last {
                    self.x_for_offset(selection.end)
                } else {
                    self.x_for_offset(self.lines[line].range.end) + self.line_height * NEWLINE_STUB
                };
                let top = self.top_for_line(line);
                fill(
                    Bounds::from_corners(point(left, top), point(right, top + self.line_height)),
                    theme::selection(),
                )
            })
            .collect()
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

fn shape_document(
    buffer: &Buffer,
    marked: Option<&Range<ByteOffset>>,
    window: &mut Window,
) -> DocumentLayout {
    let style = window.text_style();
    let composing = style.clone().highlight(HighlightStyle {
        underline: Some(UnderlineStyle {
            color: Some(style.color),
            thickness: px(1.0),
            wavy: false,
        }),
        ..HighlightStyle::default()
    });
    let font_size = style.font_size.to_pixels(window.rem_size());

    let lines = buffer
        .lines()
        .map(|(start, text)| {
            let range = start..ByteOffset(start.0 + text.len());
            let runs = line_runs(&style, &composing, &range, marked);
            let shaped = window.text_system().shape_line(
                SharedString::from(text.to_string()),
                font_size,
                &runs,
                None,
            );
            LineLayout { shaped, range }
        })
        .collect();

    DocumentLayout {
        lines,
        line_height: window.line_height(),
    }
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
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
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        #[allow(clippy::cast_precision_loss)]
        let line_count = editor.buffer.read(cx).line_count() as f32;

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * line_count).into();
        // The parent is a flex column, whose default `flex_shrink` of 1 would
        // squeeze a document taller than the window back down to it — silently
        // defeating the scroll container instead of overflowing into it.
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let selection = editor.state.selection().range();
        let cursor = editor.cursor();
        let marked = editor.state.marked_range();
        let layout = shape_document(editor.buffer.read(cx), marked.as_ref(), window);

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
        (): &mut Self::RequestLayoutState,
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
        for (index, line) in layout.lines.iter().enumerate() {
            let origin = bounds.origin + point(px(0.), layout.top_for_line(index));
            if let Err(err) = line.shaped.paint(
                origin,
                layout.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("failed to paint line {index}: {err}");
            }
        }

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

fn translate(mut quad: PaintQuad, origin: Point<Pixels>) -> PaintQuad {
    quad.bounds.origin += origin;
    quad
}
