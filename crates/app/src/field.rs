//! One line of editable text, for naming things: a `TextInput` in single-line mode.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, ContentMask, Context, CursorStyle, Element, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, InteractiveElement, IntoElement, LayoutId, Pixels,
    Render, Style, Styled, Window, actions, point, prelude::*, px, relative,
};
use inkra_editor::Selection;
use inkra_markdown::Markdown;
use inkra_text::{Buffer, ByteOffset};

use crate::editor::layout::{BlockScrolls, LayoutInputs, shape_document};
use crate::text_input::{self, FIELD_CONTEXT, InputHost, Mode, Prepainted, Quads, TextInput};
use crate::theme;
use crate::ui::Color;

actions!(field, [Confirm, Cancel]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirmed(pub(crate) String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;

/// The text changed: typed, deleted, pasted or composed. A key that changes nothing emits none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edited;

impl EventEmitter<Confirmed> for NameField {}
impl EventEmitter<Cancelled> for NameField {}
impl EventEmitter<Edited> for NameField {}

pub(crate) struct NameField {
    input: TextInput,
    /// How far the text is scrolled left; prepaint keeps the caret inside the field.
    scroll: Pixels,
}

impl NameField {
    pub(crate) fn new(initial: &str, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|_| Buffer::new(initial));
        let end = buffer.read(cx).len();
        let mut selection = Selection::at(ByteOffset::new(0));
        selection.set_head(end, inkra_editor::Goal::None);
        Self {
            input: TextInput::new(buffer, selection, Mode::SingleLine, cx),
            scroll: px(0.),
        }
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.input.buffer().read(cx).text().to_owned()
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Confirmed(self.text(cx)));
    }
}

/// The scroll nearest `scroll` that shows `caret`, in a field `width` wide over text `end` wide.
fn reveal(scroll: Pixels, caret: &Bounds<Pixels>, width: Pixels, end: Pixels) -> Pixels {
    let shown = if caret.left() < scroll {
        caret.left()
    } else if caret.right() > scroll + width {
        caret.right() - width
    } else {
        scroll
    };
    shown.clamp(px(0.), (end - width).max(px(0.)))
}

/// The text's bounds: the field's, moved left by the scroll.
fn scrolled(bounds: Bounds<Pixels>, scroll: Pixels) -> Bounds<Pixels> {
    Bounds::new(bounds.origin - point(scroll, px(0.)), bounds.size)
}

impl InputHost for NameField {
    fn input(&self) -> &TextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut TextInput {
        &mut self.input
    }

    fn after_edit(&mut self, edited: bool, _: &mut Window, cx: &mut Context<Self>) {
        if edited {
            cx.emit(Edited);
        }
    }

    fn after_move(&mut self, _: Range<ByteOffset>, _: &mut Window, _: &mut Context<Self>) {}
}

impl Focusable for NameField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.input.focus_handle().clone()
    }
}

impl Render for NameField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        text_input::on_actions(gpui::div(), cx)
            .key_context(TextInput::key_context(FIELD_CONTEXT))
            .track_focus(self.input.focus_handle())
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(Cancelled)))
            .size_full()
            .text_color(Color::Default.rgba(theme::current(cx)))
            .child(FieldElement { field: cx.entity() })
    }
}

struct FieldElement {
    field: Entity<NameField>,
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

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
        // Exactly one line box: `uniform_list` measures every row from item 0,
        // so a field even a pixel taller than a label would misplace every row below it.
        style.size.height = window.line_height().into();
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
        let input = self.field.read(cx).input();
        let selection = input.state().selection();
        let (range, head) = (selection.range(), selection.head());
        let len = input.buffer().read(cx).len();
        let marked = input.state().marked_range();
        let style = window.text_style();
        let inputs = LayoutInputs {
            buffer: input.buffer().read(cx),
            markdown: &Markdown::default(),
            selection: &range,
            marked: marked.as_ref(),
            style: &style,
            wrap_width: None,
            margin: px(0.),
            code_block_wrap: false,
            theme: theme::current(cx),
            scrolls: &BlockScrolls::default(),
        };
        let layout = shape_document(&inputs, None, window);
        let (caret, end) = (layout.caret_bounds(head), layout.caret_bounds(len).right());
        // No notify: this runs inside the frame that draws the result.
        let scroll = self.field.update(cx, |field, _| {
            field.scroll = reveal(field.scroll, &caret, bounds.size.width, end);
            field.scroll
        });
        let quads = Quads::new(&layout, &range, scrolled(bounds, scroll).origin);
        Some((Rc::new(layout), quads))
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
        if let Some((layout, quads)) = prepaint.take() {
            let text = scrolled(bounds, self.field.read(cx).scroll);
            // Scrolled text is wider than the field.
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                text_input::paint(&self.field, layout, quads, text, window, cx);
            });
        }
    }
}

#[cfg(test)]
mod tests;
