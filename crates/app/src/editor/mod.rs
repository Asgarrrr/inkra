//! The editor view: the gpui half of editing.
//!
//! Everything here needs a `Window`, font metrics, or an entity handle.
//! Anything that does not belongs in `inkra-editor`.

pub(crate) mod actions;
mod element;
mod input;
#[cfg(test)]
mod tests;

use gpui::{
    App, Bounds, Context, CursorStyle, Entity, FocusHandle, Focusable, InteractiveElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, div, px, rgb,
};
use inkra_editor::{EditorState, Goal, LogicalMovement, Selection};
use inkra_text::{Buffer, ByteOffset};

use element::{DocumentLayout, TextElement};

/// Which way a vertical movement goes. Local to this file: `inkra-editor` has
/// no notion of up and down, because neither is resolvable without metrics.
#[derive(Debug, Clone, Copy)]
pub(super) enum Vertical {
    Up,
    Down,
}

pub(crate) struct EditorView {
    buffer: Entity<Buffer>,
    state: EditorState,
    focus_handle: FocusHandle,
    is_selecting: bool,
    /// Written in `paint`, read by hit-testing, `bounds_for_range` and vertical
    /// movement. Between an edit and the next paint it describes the previous
    /// text — including, now, the previous *number of lines*. Every index into
    /// it is clamped for exactly that reason; see `DocumentLayout`.
    last_layout: Option<DocumentLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    /// gpui applies the scroll offset to this element's children in `prepaint`
    /// (`gpui/elements/div.rs:2027`), so `last_bounds` already arrives scrolled and nothing
    /// downstream subtracts the offset itself.
    scroll_handle: ScrollHandle,
    /// Held deliberately. `Subscription` unsubscribes on `Drop`, so letting
    /// this field go silently stops the view repainting when the buffer
    /// changes — with no compiler warning and no test failure short of an
    /// explicit two-view test.
    _buffer_changed: Subscription,
}

impl EditorView {
    /// Takes a buffer rather than a `&str`: where the text came from — a
    /// literal today, a file tomorrow — is not this view's business.
    pub(crate) fn new(buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let end = buffer.read(cx).len();
        let buffer_changed = cx.observe(&buffer, |_, _, cx| cx.notify());

        Self {
            buffer,
            state: EditorState::new(Selection::at(end)),
            focus_handle: cx.focus_handle(),
            is_selecting: false,
            last_layout: None,
            last_bounds: None,
            scroll_handle: ScrollHandle::new(),
            _buffer_changed: buffer_changed,
        }
    }

    /// The single path that mutates the buffer.
    ///
    /// Uses `as_mut` rather than `update`: the `GpuiBorrow` guard notifies the
    /// buffer's observers when it drops (`gpui/app.rs:3280-3286`), so a repaint
    /// cannot be lost by forgetting `cx.notify()` at a call site. `Buffer` is
    /// gpui-free by rule, so it can never notify for itself.
    fn edit_buffer<R>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R,
    ) -> R {
        let mut buffer = self.buffer.as_mut(cx);
        f(&mut self.state, &mut buffer)
    }

    /// The single path that changes view-local state without touching the text.
    ///
    /// The buffer's observers must not fire for these: marking the buffer
    /// changed when only this view's caret moved would lie to anything that
    /// subscribes to real edits — an autosave, a vault reload.
    fn update_state(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState)) {
        f(&mut self.state);
        cx.notify();
    }

    /// [`Self::update_state`] for the movements that need the buffer to
    /// resolve.
    fn move_cursor(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &Buffer)) {
        let buffer = self.buffer.read(cx);
        f(&mut self.state, buffer);
        cx.notify();
    }

    /// Up and down, the only movements that need font metrics.
    ///
    /// The column to return to is resolved here and rides in the `Selection`,
    /// so every *other* movement drops it by being made to name a [`Goal`] —
    /// including the edit path, which would otherwise leave a column describing
    /// text that has since moved.
    ///
    /// Reads `last_layout`, which describes the document as of the last paint.
    /// Its indices are clamped, so a vertical move in the window between an
    /// edit and the next frame is at worst one line off, never a panic.
    fn move_vertically(&mut self, cx: &mut Context<Self>, direction: Vertical, extend: bool) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let cursor = self.cursor();
        let line = layout.line_for_offset(cursor);

        // Captured lazily on the first vertical move and reused verbatim after
        // that: it is what brings a run of downs back out to the column it
        // started in once a short line has passed underneath.
        let goal_x = match self.state.selection().goal() {
            Goal::Column(x) => px(x),
            Goal::None => layout.x_for_offset(cursor),
        };

        let target = match direction {
            Vertical::Up => line.checked_sub(1),
            Vertical::Down => (line + 1 < layout.line_count()).then_some(line + 1),
        };
        let target = target.map(|line| layout.offset_for_x_on_line(line, goal_x));

        if let Some(offset) = target {
            let goal = Goal::Column(f32::from(goal_x));
            self.move_cursor(cx, |state, buffer| {
                state.move_vertically(buffer, offset, extend, goal);
            });
            return;
        }

        // No line that way, so this runs to the document edge instead — and lets
        // the goal go, because the next vertical move should start from the
        // column it actually landed in.
        let movement = match direction {
            Vertical::Up => LogicalMovement::DocStart,
            Vertical::Down => LogicalMovement::DocEnd,
        };
        self.move_cursor(cx, |state, buffer| state.move_(buffer, movement, extend));
    }

    fn cursor(&self) -> ByteOffset {
        self.state.cursor()
    }

    /// Unclipped, and derived from `last_layout`, which describes the text as
    /// of the last paint. Both callers hand the result to
    /// `EditorState::move_to`, which clips it.
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
        self.move_cursor(cx, |state, buffer| state.move_to(buffer, offset, extend));
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.is_selecting {
            return;
        }
        let Some(offset) = self.position_for_point(event.position) else {
            return;
        };
        self.move_cursor(cx, |state, buffer| state.move_to(buffer, offset, true));
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
    // RRGGBB hex literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping (e.g. 0x001e_1e1e) obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .id("editor")
            .key_context("EditorView")
            .role(gpui::Role::MultilineTextInput)
            .aria_label("Document")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
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
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .text_size(px(18.))
            .line_height(px(28.))
            .p_4()
            .child(TextElement {
                editor: cx.entity(),
            })
    }
}
