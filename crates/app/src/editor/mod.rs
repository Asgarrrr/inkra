pub(crate) mod actions;
mod element;
mod input;
pub(crate) mod layout;
mod scroll;
#[cfg(test)]
mod tests;

use gpui::{
    App, Bounds, Context, CursorStyle, Entity, FocusHandle, Focusable, InteractiveElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, div, point, px,
};
use inkra_editor::{EditorState, Goal, LogicalMovement, Selection};
use inkra_markdown::{Markdown, touched};
use inkra_text::{Buffer, ByteOffset};

use crate::theme;
use element::TextElement;
use layout::DocumentLayout;

#[derive(Debug, Clone, Copy)]
pub(super) enum Vertical {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RowEdge {
    Start,
    End,
}

pub(crate) struct EditorView {
    buffer: Entity<Buffer>,
    state: EditorState,
    markdown: Markdown,
    /// The `Buffer::version()` `markdown` was parsed from (live-preview spec, D10).
    parsed_at: Option<u64>,
    focus_handle: FocusHandle,
    is_selecting: bool,
    last_layout: Option<DocumentLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    /// Text top at zero scroll, as of the last paint. Never re-derive it from
    /// `last_bounds` and a live `offset()`: that counts the scroll twice.
    last_text_top: Option<Pixels>,
    /// `last_bounds` arrives already scrolled (`gpui/elements/div.rs:2027`), so
    /// no hit-test subtracts the offset.
    scroll_handle: ScrollHandle,
    /// Held for its `Drop`: losing it stops the view repainting on buffer
    /// changes, with no warning and no test failure short of a two-view test.
    _buffer_changed: Subscription,
}

impl EditorView {
    pub(crate) fn new(buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let end = buffer.read(cx).len();
        let buffer_changed = cx.observe(&buffer, Self::on_buffer_changed);

        let mut view = Self {
            buffer,
            state: EditorState::new(Selection::at(end)),
            markdown: Markdown::default(),
            parsed_at: None,
            focus_handle: cx.focus_handle(),
            is_selecting: false,
            last_layout: None,
            last_bounds: None,
            last_text_top: None,
            scroll_handle: ScrollHandle::new(),
            _buffer_changed: buffer_changed,
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
        // Rebuilt: the old `Subscription` would observe the closed note.
        self._buffer_changed = cx.observe(&buffer, Self::on_buffer_changed);
        self.buffer = buffer;
        // Reset, not compared: versions are per buffer, so two notes can share one.
        self.parsed_at = None;
        self.refresh_markdown(cx);

        self.state = EditorState::new(Selection::at(ByteOffset(0)));
        self.is_selecting = false;

        // Cleared, not kept: `reveal_cursor` would scroll by the old note's geometry.
        self.last_layout = None;
        self.last_bounds = None;
        self.last_text_top = None;
        self.scroll_handle = ScrollHandle::new();

        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
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
        let buffer = self.buffer.read(cx);
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
        self.last_bounds
    }

    /// The single path that mutates the buffer, via `as_mut`'s drop-notify.
    /// `copy` reads via `buffer.read` instead, so `⌘C` cannot mark a clean note dirty.
    fn edit_buffer<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R,
    ) -> R {
        let mut buffer = self.buffer.as_mut(cx);
        let result = f(&mut self.state, &mut buffer);
        // Load-bearing: the lease must not outlive the reshape below, which
        // reads the buffer — a second lease panics at runtime.
        drop(buffer);
        self.reshape(window, cx);
        self.reveal_cursor();
        result
    }

    fn reshape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // First: the observer only runs when effects flush, after this reshape.
        self.refresh_markdown(cx);
        let Some(last) = self.last_layout.as_ref() else {
            return;
        };
        let (style, wrap_width) = (last.style().clone(), last.wrap_width());
        let marked = self.state.marked_range();
        self.last_layout = Some(layout::shape_document(
            self.buffer.read(cx),
            &self.markdown,
            &self.state.selection().range(),
            marked.as_ref(),
            &style,
            wrap_width,
            window,
        ));
    }

    /// The buffer's observers must not fire for these: an autosave or a vault
    /// reload subscribes to real edits, not caret moves.
    fn update_state(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState)) {
        f(&mut self.state);
        cx.notify();
    }

    /// Reshapes when the move reveals or hides an element, so `last_layout` is never stale (spec D7).
    fn move_cursor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &Buffer),
    ) {
        let old = self.state.selection().range();
        f(&mut self.state, self.buffer.read(cx));
        let new = self.state.selection().range();
        let conceals = &self.markdown.conceals;
        if !touched(conceals, &old).eq(touched(conceals, &new)) {
            self.reshape(window, cx);
        }
        cx.notify();
        self.reveal_cursor();
    }

    /// Called from both gates, not the action handlers, which would forget it.
    /// Must run here: from `TextElement::prepaint` it would land a frame late.
    fn reveal_cursor(&mut self) {
        let (Some(text_top), Some(layout)) = (self.last_text_top, self.last_layout.as_ref()) else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        // Nothing prepainted yet: bounds stay default() until the first clamp.
        if viewport.size.height <= px(0.) {
            return;
        }

        let row = layout.row_for_offset(self.cursor());
        let top = text_top + layout.top_for_row(row);
        let extent = top..top + layout.row_height(row);
        let viewport = viewport.top()..viewport.bottom();

        if let Some(offset) = scroll::reveal(
            extent,
            viewport,
            self.scroll_handle.offset().y,
            layout.body_row_height(),
        ) {
            self.scroll_handle
                .set_offset(point(self.scroll_handle.offset().x, offset));
        }
    }

    fn move_vertically(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        direction: Vertical,
        extend: bool,
    ) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let cursor = self.cursor();
        let row = layout.row_for_offset(cursor);

        // Reused verbatim: re-widens a run of downs after a short row (x is
        // from the row's start, so it means the same on every row).
        let goal_x = match self.state.selection().goal() {
            Goal::Column(x) => px(x),
            Goal::None => layout.x_for_offset(cursor),
        };

        let step = |row: usize| match direction {
            Vertical::Up => row.checked_sub(1),
            Vertical::Down => (row + 1 < layout.row_count()).then_some(row + 1),
        };
        let mut target = step(row);
        while let Some(candidate) = target
            && !layout.holds_caret(candidate)
        {
            target = step(candidate);
        }
        let target = target.map(|row| layout.offset_on_row(row, goal_x));

        if let Some(offset) = target {
            let goal = Goal::Column(f32::from(goal_x));
            self.move_cursor(window, cx, |state, buffer| {
                state.move_vertically(buffer, offset, extend, goal);
            });
            return;
        }

        // Drops the goal: the next vertical move starts from where this landed, not where it aimed.
        let movement = match direction {
            Vertical::Up => LogicalMovement::DocStart,
            Vertical::Down => LogicalMovement::DocEnd,
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_(buffer, movement, extend);
        });
    }

    fn move_to_row_edge(&mut self, window: &mut Window, cx: &mut Context<Self>, edge: RowEdge) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let row = layout.row_bounds(layout.row_for_offset(self.cursor()));
        let target = match edge {
            RowEdge::Start => *row.start(),
            RowEdge::End => *row.end(),
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, target, false);
        });
    }

    pub(crate) fn cursor(&self) -> ByteOffset {
        self.state.cursor()
    }

    fn position_for_point(&self, position: Point<Pixels>) -> Option<ByteOffset> {
        let (bounds, layout) = (self.last_bounds?, self.last_layout.as_ref()?);
        Some(layout.offset_for_position(position - bounds.origin))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let Some(offset) = self.position_for_point(event.position) else {
            return;
        };
        let extend = event.modifiers.shift;
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, offset, extend);
        });
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting {
            return;
        }
        let Some(offset) = self.position_for_point(event.position) else {
            return;
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, offset, true);
        });
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .id("editor")
            .key_context("EditorView")
            .role(gpui::Role::MultilineTextInput)
            .aria_label("Document")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
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
