pub(crate) mod actions;
mod element;
pub(crate) mod layout;
mod motion;
mod mouse;
mod parked;
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
use inkra_text::{Bias, Buffer, ByteOffset, Version};

use crate::open::Open;
use crate::text_input::{self, InputHost, Mode, TextInput};
use crate::theme;
use element::TextElement;
use viewport::{Reveal, ScrollTo, Viewport};

pub(crate) use parked::ParkedView;
pub(crate) use viewport::TopLine;

/// A link the user asked to follow: ⌘-click or ⌘↵ here, ⌘⇧-click or ⌘⇧↵ in a new tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkFollowed {
    pub(crate) link: Link,
    pub(crate) open: Open,
}

impl EventEmitter<LinkFollowed> for EditorView {}

/// What a tab puts back on its note: the selection and the text on top (tabs spec, A2).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewState {
    pub(crate) selection: Selection,
    pub(crate) top: TopLine,
}

impl ViewState {
    /// Onto `buffer`'s graphemes, tail then head, so direction and goal survive.
    fn clipped(self, buffer: &Buffer) -> Self {
        let clip = |offset| buffer.clip_grapheme(offset, Bias::Left);
        let mut selection = Selection::at(clip(self.selection.tail()));
        selection.set_head(clip(self.selection.head()), self.selection.goal());
        Self {
            selection,
            top: self.top.clipped(buffer),
        }
    }
}

pub(crate) struct EditorView {
    input: TextInput,
    markdown: Markdown,
    /// The `Buffer::version()` `markdown` was parsed from (live-preview spec, D10).
    parsed_at: Option<Version>,
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
    pub(crate) fn open_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        view: Option<ViewState>,
        cx: &mut Context<Self>,
    ) {
        let view = view.map(|view| view.clipped(buffer.read(cx)));
        // Rebuilt: the old `Subscription`s would observe the closed note.
        self._buffer = Self::watch_buffer(&buffer, cx);
        let (selection, top) = view.map_or((Selection::at(ByteOffset::new(0)), None), |view| {
            (view.selection, Some(view.top))
        });
        // Clears the layout too: `reveal_cursor` would scroll by the old note's geometry.
        self.input.rebind(buffer, selection);
        // Reset, not compared: versions are per buffer, so two notes can share one.
        self.parsed_at = None;
        self.refresh_markdown(cx);

        self.is_selecting = false;
        self.viewport = Viewport::default();
        // Never a reveal: the restored top line is where the reader left it.
        if let Some(top) = top {
            self.viewport.request(ScrollTo::Top(top));
        }

        cx.notify();
    }

    fn watch_buffer(buffer: &Entity<Buffer>, cx: &mut Context<Self>) -> [Subscription; 2] {
        [
            cx.observe(buffer, Self::on_buffer_changed),
            cx.subscribe(buffer, Self::on_buffer_reloaded),
        ]
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

    fn reshape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // First: the observer only runs when effects flush, after this reshape.
        self.refresh_markdown(cx);
        let Some(last) = self.input.last_layout() else {
            return;
        };
        let (style, wrap_width) = (last.style().clone(), last.wrap_width());
        let state = self.input.state();
        let (selection, marked) = (state.selection().range(), state.marked_range());
        let inputs = layout::LayoutInputs {
            buffer: self.input.buffer().read(cx),
            markdown: &self.markdown,
            selection: &selection,
            marked: marked.as_ref(),
            style: &style,
            wrap_width,
            theme: theme::current(cx),
        };
        // A whole-document shape: skipped when the inputs still give this layout.
        if last.is_shaped_from(&inputs) {
            return;
        }
        let layout = Rc::new(layout::shape_document(&inputs, Some(last), window));
        self.input.set_layout(layout);
    }

    pub(crate) fn cursor(&self) -> ByteOffset {
        self.input.cursor()
    }

    /// Seals the undo group even with no edit, so typing on both sides of a tab switch undoes in two steps.
    pub(crate) fn capture(&self, cx: &mut App) -> ViewState {
        // `update`, never `as_mut`: a lease notifies on drop, which schedules an autosave.
        self.input
            .buffer()
            .update(cx, |buffer, _| buffer.finalize_transaction());
        self.view_state()
    }

    /// A view back onto the bound buffer: no resubscribe and no reparse, unlike `open_buffer`.
    pub(crate) fn restore(&mut self, view: ViewState, cx: &mut Context<Self>) {
        let view = view.clipped(self.input.buffer().read(cx));
        self.input.reset_state(view.selection);
        self.is_selecting = false;
        self.viewport = Viewport::default();
        self.viewport.request(ScrollTo::Top(view.top));
        cx.notify();
    }

    /// Reads a waiting scroll request before the drawn frame, so two switches in one frame keep it.
    pub(crate) fn view_state(&self) -> ViewState {
        ViewState {
            selection: self.input.state().selection().clone(),
            top: self
                .viewport
                .capture(self.input.last_layout().map(Rc::as_ref), self.cursor()),
        }
    }

    fn reveal_cursor(&mut self, how: Reveal) {
        let cursor = self.cursor();
        let layout = self.input.last_layout().map(Rc::as_ref);
        self.viewport.reveal(layout, cursor, how);
    }
}

#[cfg(test)]
impl EditorView {
    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        self.input.buffer()
    }

    pub(crate) fn parsed_at(&self) -> Option<Version> {
        self.parsed_at
    }

    pub(crate) fn selected(&self) -> std::ops::Range<ByteOffset> {
        self.input.state().selection().range()
    }

    fn spans(&self) -> &[inkra_markdown::Span] {
        &self.markdown.spans
    }

    pub(crate) fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.input.last_bounds()
    }

    fn drawn_layout(&self) -> &layout::DocumentLayout {
        let Some(layout) = self.input.last_layout() else {
            panic!("the editor has not been drawn");
        };
        layout
    }

    /// Scrolls so the row holding `offset` starts at the viewport's top; takes effect at the next draw.
    pub(crate) fn scroll_to_top(&mut self, offset: ByteOffset, cx: &mut Context<Self>) {
        self.viewport.scroll_row_to_top(self.drawn_layout(), offset);
        cx.notify();
    }

    /// The offset at the viewport's top-left, read from the last layout.
    pub(crate) fn offset_at_top(&self) -> ByteOffset {
        self.viewport.offset_at_top(self.drawn_layout())
    }

    pub(crate) fn scroll_y(&self) -> Pixels {
        self.viewport.handle().offset().y
    }

    /// Whether the caret's whole row lies inside the viewport, read from the last layout.
    pub(crate) fn caret_in_view(&self) -> bool {
        self.viewport.shows_row(self.drawn_layout(), self.cursor())
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // A reload applied without a buffer notify would otherwise shape stale spans.
        self.refresh_markdown(cx);
        let colors = theme::current(cx).colors;
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
            .on_action(cx.listener(Self::follow_link_in_new_tab))
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
            .bg(colors.editor_background)
            .text_color(colors.md_text)
            .text_size(theme::BODY_TEXT_SIZE)
            .line_height(theme::BODY_LINE_HEIGHT)
            .p_4()
            // The only child: its prepaint moves this div's offset and shifts only itself.
            .child(TextElement {
                editor: cx.entity(),
            })
    }
}
