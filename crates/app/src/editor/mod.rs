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
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render,
    ShapedLine, StatefulInteractiveElement, Styled, Subscription, Window, div, px, rgb,
};
use inkra_editor::{EditorState, Selection};
use inkra_text::{Buffer, ByteOffset};

use element::TextElement;

pub(crate) struct EditorView {
    buffer: Entity<Buffer>,
    state: EditorState,
    focus_handle: FocusHandle,
    is_selecting: bool,
    /// Written in `paint`, read by hit-testing and `bounds_for_range`. Between
    /// an edit and the next paint they describe the previous text.
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
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
            _buffer_changed: buffer_changed,
        }
    }

    /// The single path that mutates the buffer.
    ///
    /// Uses `as_mut` rather than `update`: the `GpuiBorrow` guard notifies the
    /// buffer's observers when it drops (`app.rs:2415-2422`), so a repaint
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
    /// resolve. Also the place the goal column will be reset from once up and
    /// down exist: that state needs font metrics, so it lives here while the
    /// selection it qualifies lives in `inkra-editor`.
    fn move_cursor(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &Buffer)) {
        let buffer = self.buffer.read(cx);
        f(&mut self.state, buffer);
        cx.notify();
    }

    fn cursor(&self) -> ByteOffset {
        self.state.cursor()
    }

    /// Unclipped, and derived from `last_layout`, which describes the text as
    /// of the last paint. Both callers hand the result to
    /// `EditorState::move_to`, which clips it.
    fn position_for_x(&self, x: Pixels) -> Option<ByteOffset> {
        let (bounds, line) = (self.last_bounds?, self.last_layout.as_ref()?);
        Some(ByteOffset(line.closest_index_for_x(x - bounds.left())))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let Some(offset) = self.position_for_x(event.position.x) else {
            return;
        };
        let extend = event.modifiers.shift;
        self.move_cursor(cx, |state, buffer| state.move_to(buffer, offset, extend));
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.is_selecting {
            return;
        }
        let Some(offset) = self.position_for_x(event.position.x) else {
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
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .flex_col()
            .size_full()
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
