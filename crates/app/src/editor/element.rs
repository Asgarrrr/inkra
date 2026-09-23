//! `TextElement`: the gpui element that measures, paints and hit-tests the editor.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, LayoutId,
    PaintQuad, Pixels, Point, Style, TextStyle, Window, fill, prelude::*, px, relative, size,
};

use super::EditorView;
use super::layout::{DocumentLayout, shape_document};
use crate::theme;

pub(super) struct TextElement {
    pub(super) editor: Entity<EditorView>,
}

pub(super) struct PrepaintState {
    layout: Option<DocumentLayout>,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
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
                if let Some(layout) = stash
                    .iter()
                    .find(|layout| layout.wrap_width() == Some(width))
                {
                    return size(reported, layout.height());
                }
                let editor = editor.read(cx);
                let marked = editor.state.marked_range();
                let layout = shape_document(
                    editor.buffer.read(cx),
                    editor.markdown(),
                    &editor.state.selection().range(),
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
                editor.markdown(),
                &editor.state.selection().range(),
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
                Bounds::new(bounds.origin + at, size(px(2.), layout.line_height())),
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
        layout.wrap_width().map_or(f32::INFINITY, |measured| {
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
