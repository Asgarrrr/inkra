pub(crate) mod actions;
mod element;
pub(crate) mod layout;
mod motion;
mod mouse;
mod reload;
#[cfg(test)]
mod tests;
mod viewport;

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Context, CursorStyle, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    div,
};
#[cfg(test)]
use gpui::{Bounds, Pixels};
use inkra_editor::Selection;
use inkra_markdown::{Link, Markdown, touched};
use inkra_text::{Buffer, ByteOffset};

use crate::text_input::{self, InputHost, Mode, TextInput};
use crate::theme;
use element::TextElement;
use viewport::{Reveal, Viewport};

/// A link the user asked to follow, with ⌘-click or ⌘↵.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkFollowed(pub(crate) Link);

impl EventEmitter<LinkFollowed> for EditorView {}

pub(crate) struct EditorView {
    input: TextInput,
    markdown: Markdown,
    /// The `Buffer::version()` `markdown` was parsed from (live-preview spec, D10).
    parsed_at: Option<u64>,
    is_selecting: bool,
    viewport: Viewport,
    /// Held for its `Drop`: losing it stops the view repainting on buffer changes, with
    /// no test failure short of a two-view test, and leaves a reload's caret on the old text.
    _buffer: [Subscription; 2],
}

impl EditorView {
    pub(crate) fn new(buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let end = buffer.read(cx).len();
        let subscriptions = Self::watch_buffer(&buffer, cx);

        let mut view = Self {
            input: TextInput::new(buffer, Selection::at(end), Mode::Multiline, cx),
            markdown: Markdown::default(),
            parsed_at: None,
            is_selecting: false,
            viewport: Viewport::default(),
            _buffer: subscriptions,
        };
        view.refresh_markdown(cx);
        view
    }

    /// The only gate that may rebind the buffer: through `edit_buffer` instead,
    /// the new note's text would land on the *previous* note's undo stack.
    #[expect(
        clippy::used_underscore_binding,
        reason = "resubscribing is the point of the gate"
    )]
    pub(crate) fn open_buffer(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        // Rebuilt: the old `Subscription`s would observe the closed note.
        self._buffer = Self::watch_buffer(&buffer, cx);
        // Clears the caret and the layout too: `reveal_cursor` would scroll by the old note's geometry.
        self.input.rebind(buffer);
        // Reset, not compared: versions are per buffer, so two notes can share one.
        self.parsed_at = None;
        self.refresh_markdown(cx);

        self.is_selecting = false;
        self.viewport = Viewport::default();

        cx.notify();
    }

    fn watch_buffer(buffer: &Entity<Buffer>, cx: &mut Context<Self>) -> [Subscription; 2] {
        [
            cx.observe(buffer, Self::on_buffer_changed),
            cx.subscribe(buffer, Self::on_buffer_reloaded),
        ]
    }

    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        self.input.buffer()
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> std::ops::Range<ByteOffset> {
        self.input.state().selection().range()
    }

    #[cfg(test)]
    fn spans(&self) -> &[inkra_markdown::Span] {
        &self.markdown.spans
    }

    fn markdown(&self) -> &Markdown {
        &self.markdown
    }

    /// Parses only when the buffer moved since the last parse; a caret move costs nothing.
    fn refresh_markdown(&mut self, cx: &App) {
        let buffer = self.input.buffer().read(cx);
        let version = buffer.version();
        if self.parsed_at == Some(version) {
            return;
        }
        self.markdown = inkra_markdown::parse(buffer.text());
        self.parsed_at = Some(version);
    }

    /// Covers writes that bypass `edit_buffer`; tests make them today.
    fn on_buffer_changed(&mut self, _: Entity<Buffer>, cx: &mut Context<Self>) {
        self.refresh_markdown(cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.input.last_bounds()
    }

    #[cfg(test)]
    fn drawn_layout(&self) -> &layout::DocumentLayout {
        let Some(layout) = self.input.last_layout() else {
            panic!("the editor has not been drawn");
        };
        layout
    }

    /// Scrolls so the row holding `offset` starts at the viewport's top; takes effect at the next draw.
    #[cfg(test)]
    pub(crate) fn scroll_to_top(&mut self, offset: ByteOffset, cx: &mut Context<Self>) {
        self.viewport.scroll_row_to_top(self.drawn_layout(), offset);
        cx.notify();
    }

    /// The offset at the viewport's top-left, read from the last layout.
    #[cfg(test)]
    pub(crate) fn offset_at_top(&self) -> ByteOffset {
        self.viewport.offset_at_top(self.drawn_layout())
    }

    #[cfg(test)]
    pub(crate) fn scroll_y(&self) -> Pixels {
        self.viewport.handle().offset().y
    }

    /// Whether the caret's whole row lies inside the viewport, read from the last layout.
    #[cfg(test)]
    pub(crate) fn caret_in_view(&self) -> bool {
        self.viewport.shows_row(self.drawn_layout(), self.cursor())
    }

    fn reshape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // First: the observer only runs when effects flush, after this reshape.
        self.refresh_markdown(cx);
        let Some(last) = self.input.last_layout() else {
            return;
        };
        let (style, wrap_width) = (last.style().clone(), last.wrap_width());
        let state = self.input.state();
        let (selection, marked) = (state.selection().range(), state.marked_range());
        let buffer = self.input.buffer().read(cx);
        // A whole-document shape: skipped when the inputs still give this layout.
        if last.is_shaped_from(
            buffer,
            &self.markdown,
            &selection,
            marked.as_ref(),
            &style,
            wrap_width,
        ) {
            return;
        }
        let layout = Rc::new(layout::shape_document(
            buffer,
            &self.markdown,
            &selection,
            marked.as_ref(),
            &style,
            wrap_width,
            window,
        ));
        self.input.set_layout(layout);
    }

    pub(crate) fn cursor(&self) -> ByteOffset {
        self.input.cursor()
    }

    fn reveal_cursor(&mut self, how: Reveal) {
        let cursor = self.cursor();
        let layout = self.input.last_layout().map(Rc::as_ref);
        self.viewport.reveal(layout, cursor, how);
    }

    /// From `render`, before prepaint reads the offset, else the scroll lands a frame late.
    fn restore_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(to) = self.viewport.take_pending() else {
            return;
        };
        // A reload may have landed since the last paint shaped `last_layout`.
        self.reshape(window, cx);
        let cursor = self.cursor();
        let layout = self.input.last_layout().map(Rc::as_ref);
        self.viewport.apply(to, layout, cursor);
    }
}

impl InputHost for EditorView {
    fn input(&self) -> &TextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut TextInput {
        &mut self.input
    }

    /// Reshapes with the style stored in the layout: outside a draw, `window.text_style()` is gpui's default.
    fn after_edit(&mut self, _: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.reshape(window, cx);
        self.reveal_cursor(Reveal::Minimal);
    }

    /// Reshapes when the move reveals or hides an element, so the layout is never stale (spec D7).
    fn after_move(&mut self, old: Range<ByteOffset>, window: &mut Window, cx: &mut Context<Self>) {
        let new = self.input.state().selection().range();
        let conceals = &self.markdown.conceals;
        if !touched(conceals, &old).eq(touched(conceals, &new)) {
            self.reshape(window, cx);
        }
        self.reveal_cursor(Reveal::Minimal);
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.input.focus_handle().clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.restore_scroll(window, cx);
        text_input::on_actions(div(), cx)
            .id("editor")
            .key_context(TextInput::key_context(text_input::EDITOR_CONTEXT))
            .role(gpui::Role::MultilineTextInput)
            .aria_label("Document")
            .track_focus(self.input.focus_handle())
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::follow_link))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .overflow_y_scroll()
            .track_scroll(self.viewport.handle())
            .bg(theme::editor_background())
            .text_color(theme::editor_text())
            .text_size(theme::BODY_TEXT_SIZE)
            .line_height(theme::BODY_LINE_HEIGHT)
            .p_4()
            .child(TextElement {
                editor: cx.entity(),
            })
    }
}
