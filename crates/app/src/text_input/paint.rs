//! The element half every host shares: selection and caret quads, then the paint tail.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Bounds, Entity, PaintQuad, Pixels, Point, Window, fill};
use inkra_text::ByteOffset;

use super::{InputAdapter, InputHost};
use crate::editor::layout::DocumentLayout;

/// What a host's prepaint hands to `paint`.
pub(crate) type Prepainted = Option<(Rc<DocumentLayout>, Quads)>;

pub(crate) struct Quads {
    selection: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl Quads {
    /// A caret for an empty selection, else one quad per row; `origin` is the text's.
    pub(crate) fn new(
        layout: &DocumentLayout,
        selection: &Range<ByteOffset>,
        origin: Point<Pixels>,
    ) -> Self {
        if selection.is_empty() {
            let caret = layout.caret_bounds(selection.start);
            return Self {
                selection: Vec::new(),
                caret: Some(fill(
                    Bounds::new(origin + caret.origin, caret.size),
                    layout.theme().colors.accent,
                )),
            };
        }
        let selection = layout
            .selection_quads(selection)
            .into_iter()
            .map(|mut quad| {
                quad.bounds.origin += origin;
                quad
            })
            .collect();
        Self {
            selection,
            caret: None,
        }
    }
}

/// Input handler, selection, text, caret when focused, then the layout and
/// bounds every pixel ↔ offset call reads until the next paint.
pub(crate) fn paint<V: InputHost>(
    view: &Entity<V>,
    layout: Rc<DocumentLayout>,
    quads: Quads,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let focus_handle = view.read(cx).input().focus_handle().clone();
    window.handle_input(&focus_handle, InputAdapter::new(bounds, view.clone()), cx);

    layout.paint_backgrounds(bounds.origin, window);
    for quad in quads.selection {
        window.paint_quad(quad);
    }
    layout.paint(bounds.origin, window, cx);
    if focus_handle.is_focused(window)
        && let Some(caret) = quads.caret
    {
        window.paint_quad(caret);
    }

    view.update(cx, |view, _| view.input_mut().store(layout, bounds));
}
