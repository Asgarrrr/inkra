//! The element half every host shares: selection and caret quads, then the paint tail.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Bounds, Entity, PaintQuad, Pixels, Point, Window, fill};
use inkra_text::ByteOffset;

use super::{InputAdapter, InputHost};
use crate::editor::layout::DocumentLayout;

#[cfg(test)]
thread_local! {
    static PAINTED_CARET: std::cell::Cell<Option<Bounds<Pixels>>> =
        const { std::cell::Cell::new(None) };
}

/// The caret the last paint on this thread drew, in window coordinates.
#[cfg(test)]
pub(crate) fn painted_caret() -> Option<Bounds<Pixels>> {
    PAINTED_CARET.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! {
    static PAINTED_SELECTION: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many selection quads the last paint on this thread drew.
#[cfg(test)]
pub(crate) fn painted_selection() -> usize {
    PAINTED_SELECTION.with(std::cell::Cell::get)
}

/// What a host's prepaint hands to `paint`.
pub(crate) type Prepainted = Option<(Rc<DocumentLayout>, Quads)>;

pub(crate) struct Quads {
    selection: Vec<PaintQuad>,
    bars: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl Quads {
    /// A caret for an empty selection, unless its block scrolled it out, else one quad per row
    /// reaching `visible`; `origin` is the text's, `visible` a span of layout `y`.
    pub(crate) fn new(
        layout: &DocumentLayout,
        selection: &Range<ByteOffset>,
        origin: Point<Pixels>,
        visible: &Range<Pixels>,
    ) -> Self {
        if selection.is_empty() {
            let caret = layout.caret_bounds(selection.start);
            return Self {
                selection: Vec::new(),
                bars: Vec::new(),
                caret: layout.caret_visible(selection.start).then(|| {
                    fill(
                        Bounds::new(origin + caret.origin, caret.size),
                        layout.theme().colors.accent,
                    )
                }),
            };
        }
        let selection = layout
            .selection_quads(selection, visible)
            .into_iter()
            .map(|mut quad| {
                quad.bounds.origin += origin;
                quad
            })
            .collect();
        Self {
            selection,
            bars: Vec::new(),
            caret: None,
        }
    }

    /// Code block scroll bars, painted over the text and under the caret.
    pub(crate) fn with_bars(self, bars: Vec<PaintQuad>) -> Self {
        Self { bars, ..self }
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
    #[cfg(test)]
    PAINTED_SELECTION.with(|painted| painted.set(quads.selection.len()));
    for quad in quads.selection {
        window.paint_quad(quad);
    }
    layout.paint(bounds.origin, window, cx);
    for bar in quads.bars {
        window.paint_quad(bar);
    }
    let caret = quads.caret.filter(|_| focus_handle.is_focused(window));
    #[cfg(test)]
    PAINTED_CARET.with(|painted| painted.set(caret.as_ref().map(|caret| caret.bounds)));
    if let Some(caret) = caret {
        window.paint_quad(caret);
    }

    view.update(cx, |view, _| view.input_mut().store(layout, bounds));
}
