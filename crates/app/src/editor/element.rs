//! Layout and painting.
//!
//! Single-line: `shape_line` asserts the text contains no newline
//! (`text_system.rs:373`). Multi-line shapes per logical line instead.

use gpui::{
    App, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, HighlightStyle, LayoutId,
    PaintQuad, Pixels, ShapedLine, SharedString, Style, TextAlign, TextRun, UnderlineStyle, Window,
    fill, point, prelude::*, px, relative, rgba, size,
};

use super::EditorView;

pub(super) struct TextElement {
    pub(super) editor: Entity<EditorView>,
}

pub(super) struct PrepaintState {
    line: Option<ShapedLine>,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    #[allow(clippy::unreadable_literal)]
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
        let text = SharedString::from(editor.buffer.read(cx).text().to_string());
        let selection = editor.state.selection().range();
        let cursor = editor.cursor();
        let marked = editor.state.marked_range();

        let style = window.text_style();
        let runs: Vec<TextRun> = match marked {
            Some(marked) => {
                let composing = style.clone().highlight(HighlightStyle {
                    underline: Some(UnderlineStyle {
                        color: Some(style.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..HighlightStyle::default()
                });
                [
                    style.to_run(marked.start.0),
                    composing.to_run(marked.end.0 - marked.start.0),
                    style.to_run(text.len() - marked.end.0),
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            }
            None => vec![style.to_run(text.len())],
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(text, font_size, &runs, None);

        let (selection, cursor) = if selection.is_empty() {
            let quad = fill(
                Bounds::new(
                    point(bounds.left() + line.x_for_index(cursor.0), bounds.top()),
                    size(px(2.), bounds.bottom() - bounds.top()),
                ),
                rgba(0x4ea1ffff),
            );
            (None, Some(quad))
        } else {
            let quad = fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(selection.start.0),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(selection.end.0),
                        bounds.bottom(),
                    ),
                ),
                rgba(0x3311ff55),
            );
            (Some(quad), None)
        };

        PrepaintState {
            line: Some(line),
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

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let Some(line) = prepaint.line.take() else {
            return;
        };
        if let Err(err) = line.paint(
            bounds.origin,
            window.line_height(),
            TextAlign::Left,
            None,
            window,
            cx,
        ) {
            eprintln!("failed to paint line: {err}");
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.editor.update(cx, |editor, _| {
            editor.last_layout = Some(line);
            editor.last_bounds = Some(bounds);
        });
    }
}
