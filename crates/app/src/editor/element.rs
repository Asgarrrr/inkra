//! `TextElement`: the gpui element that measures, paints and hit-tests the editor.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ElementId, Entity, GlobalElementId, LayoutId, Pixels, Style,
    TextStyle, Window, point, prelude::*, px, relative, size,
};
use inkra_markdown::Styles;

use super::EditorView;
use super::layout::{DocumentLayout, shape_document};
use crate::text_input::{self, Prepainted, Quads};
use crate::theme;

pub(super) struct TextElement {
    pub(super) editor: Entity<EditorView>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Every layout shaped this frame, keyed by width: taffy may call the measure
/// closure several times and answer its final pass from a cache, so the last call isn't necessarily final.
type Stash = Rc<RefCell<Vec<Rc<DocumentLayout>>>>;

impl Element for TextElement {
    type RequestLayoutState = (Stash, Inset);
    type PrepaintState = EditorPrepaint;

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
        let inset = Inset {
            column: column_width(&text_style, window),
            margin: margin_width(&text_style, window),
        };
        // Wide enough to centre the column between two margins; D12 reserves only the left one.
        let widest = inset.column + inset.margin * 2.;

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.max_size.width = widest.into();
        // Flex column's default flex_shrink of 1 would squeeze an overtall
        // document back down instead of overflowing into the scroll container.
        style.flex_shrink = 0.;

        let stash = Stash::default();
        let editor = self.editor.clone();
        let measured = stash.clone();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, cx| {
                // The sizing pass hands `known` unclamped by max_size; only the
                // layout pass clamps, so the widest width is reapplied here.
                let definite = known
                    .width
                    .or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                    .map(|width| width.min(widest));

                // Wrapped text has no minimum width; reporting a width here
                // would become the flex item's automatic minimum and push it past a narrower pane.
                let reported = match (known.width, available.width) {
                    (None, AvailableSpace::MinContent) => px(0.),
                    _ => definite.unwrap_or(widest),
                };

                let editor = editor.read(cx);
                // An intrinsic query asks only for a width, so it reuses the last one's layout.
                let text = definite.map_or_else(
                    || {
                        editor
                            .input
                            .last_layout()
                            .and_then(|layout| layout.wrap_width())
                            .unwrap_or_else(|| inset.text_width(widest))
                    },
                    |width| inset.text_width(width),
                );
                let mut stash = measured.borrow_mut();
                if let Some(layout) = stash
                    .iter()
                    .find(|layout| layout.wrap_width() == Some(text))
                {
                    return size(reported, layout.height());
                }
                let buffer = editor.input.buffer().read(cx);
                let selection = editor.input.state().selection().range();
                let marked = editor.input.state().marked_range();
                let layout = match editor.input.last_layout().filter(|layout| {
                    layout.is_shaped_from(
                        buffer,
                        editor.markdown(),
                        &selection,
                        marked.as_ref(),
                        &text_style,
                        Some(text),
                    )
                }) {
                    Some(last) => Rc::clone(last),
                    None => Rc::new(shape_document(
                        buffer,
                        editor.markdown(),
                        &selection,
                        marked.as_ref(),
                        &text_style,
                        Some(text),
                        window,
                    )),
                };
                let height = layout.height();
                stash.push(layout);
                size(reported, height)
            });
        (layout_id, (stash, inset))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (stash, inset): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut text = inset.text_bounds(bounds);
        let layout = take_measured(stash, text.size.width).unwrap_or_else(|| {
            // One frame stale height; corrects next frame. gpui's own `Text`
            // panics here (`elements/text.rs:782-790`) — a note editor must not.
            eprintln!("no layout measured at width {}", text.size.width);
            let editor = self.editor.read(cx);
            let marked = editor.input.state().marked_range();
            let style = window.text_style();
            Rc::new(shape_document(
                editor.input.buffer().read(cx),
                editor.markdown(),
                &editor.input.state().selection().range(),
                marked.as_ref(),
                &style,
                Some(text.size.width),
                window,
            ))
        });

        let applied = window.element_offset().y;
        let moved = self.editor.update(cx, |editor, _| {
            let cursor = editor.cursor();
            editor.viewport.resolve(text.origin.y, &layout, cursor)
        });
        // `layout_bounds` snaps the element offset (`window.rs:5107`); so does the shift.
        text.origin.y += window.pixel_snap(applied + moved) - window.pixel_snap(applied);

        let selection = self.editor.read(cx).input.state().selection().range();
        let quads = Quads::new(&layout, &selection, text.origin);
        EditorPrepaint {
            painted: Some((layout, quads)),
            text,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((layout, quads)) = prepaint.painted.take() else {
            return;
        };
        // The shifted bounds, not the parameter: hit tests and the IME read them until the next paint.
        text_input::paint(&self.editor, layout, quads, prepaint.text, window, cx);
    }
}

pub(super) struct EditorPrepaint {
    painted: Prepainted,
    /// Shifted by the offset prepaint resolved, so paint places the text where the next frame will.
    text: Bounds<Pixels>,
}

/// The measured layout nearest the element's width — not equal, since bounds
/// are pixel-snapped after taffy measured them.
fn take_measured(stash: &Stash, width: Pixels) -> Option<Rc<DocumentLayout>> {
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

/// The widest opening heading marker, `#` × n and a space, over levels 1 to 6.
pub(super) fn margin_width(style: &TextStyle, window: &mut Window) -> Pixels {
    (1..=6)
        .map(|level| {
            let marker = format!("{} ", "#".repeat(usize::from(level)));
            let run = theme::markdown_style(style, Styles::heading(level) | Styles::MARKER)
                .to_run(marker.len());
            let size = theme::heading_metrics(level).size;
            window
                .text_system()
                .shape_line(marker.into(), size, &[run], None)
                .width()
        })
        .fold(px(0.), Pixels::max)
        // Whole pixels: the element's origin is snapped, so the text keeps the column's old x.
        .ceil()
}

/// Where the text sits in the element (live-preview spec, D12).
#[derive(Clone, Copy)]
pub(super) struct Inset {
    column: Pixels,
    margin: Pixels,
}

impl Inset {
    /// The parent centres the element, capped at `column + 2 × margin`, so the text starts one
    /// margin in; a pane narrower than the margin leaves it no width.
    fn text_width(self, element: Pixels) -> Pixels {
        self.column.min(element - self.margin).max(px(0.))
    }

    fn text_bounds(self, element: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            point(element.origin.x + self.margin, element.origin.y),
            size(self.text_width(element.size.width), element.size.height),
        )
    }
}
