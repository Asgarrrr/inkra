//! The editor view: the gpui half of editing. Everything here needs a `Window`,
//! font metrics, or an entity handle; anything else belongs in `inkra-editor`.

pub(crate) mod actions;
mod element;
mod input;
mod scroll;
#[cfg(test)]
mod tests;

use gpui::{
    App, Bounds, Context, CursorStyle, Entity, FocusHandle, Focusable, InteractiveElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, div, point, px,
    rgb,
};
use inkra_editor::{EditorState, Goal, LogicalMovement, Selection};
use inkra_text::{Buffer, ByteOffset};

use element::{DocumentLayout, TextElement};

/// Local: up and down are not resolvable without font metrics.
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
    /// Written in `paint`, so between an edit and the next paint it describes
    /// the previous text. Every index into it clamps; see `DocumentLayout`.
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
        let buffer_changed = cx.observe(&buffer, |_, _, cx| cx.notify());

        Self {
            buffer,
            state: EditorState::new(Selection::at(end)),
            focus_handle: cx.focus_handle(),
            is_selecting: false,
            last_layout: None,
            last_bounds: None,
            last_text_top: None,
            scroll_handle: ScrollHandle::new(),
            _buffer_changed: buffer_changed,
        }
    }

    /// The only gate that may rebind the buffer. Through `edit_buffer` instead,
    /// the new note's text lands on the *previous* note's undo stack and `⌘Z`
    /// restores one file into another. Resets every mutable field but
    /// `focus_handle`; only `state` and `_buffer_changed` carry a regression
    /// test (`a_swap_resets_the_caret_and_any_ime_composition`,
    /// `editing_after_a_swap_still_notifies`).
    // Renaming `_buffer_changed` trades this lint for "never read": rustc
    // exempts `_`-prefixed fields from the latter.
    #[expect(
        clippy::used_underscore_binding,
        reason = "resubscribing is the point of the gate"
    )]
    pub(crate) fn open_buffer(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        // Rebuilt: the old `Subscription` would observe the closed note.
        self._buffer_changed = cx.observe(&buffer, |_, _, cx| cx.notify());
        self.buffer = buffer;

        // Offset 0, unlike `new`, which seats the caret at `end`.
        self.state = EditorState::new(Selection::at(ByteOffset(0)));
        self.is_selecting = false;

        // Cleared, not kept: `reveal_line` would scroll the new note by the
        // old note's geometry.
        self.last_layout = None;
        self.last_bounds = None;
        self.last_text_top = None;
        self.scroll_handle = ScrollHandle::new();

        cx.notify();
    }

    /// `#[cfg(test)]`: no production caller reads the buffer back out.
    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
    }

    /// The single path that mutates the buffer. `as_mut`, not `update`: it
    /// notifies on drop (`gpui/app.rs:3280-3286`), and `Buffer` is gpui-free by
    /// rule, so nothing else can.
    fn edit_buffer<R>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R,
    ) -> R {
        let mut buffer = self.buffer.as_mut(cx);
        let result = f(&mut self.state, &mut buffer);
        let line = buffer.line_for_offset(self.state.cursor());
        // Load-bearing: the lease would outlive `reveal_line`, and a second
        // lease panics at runtime (`gpui/app/entity_map.rs:231`).
        drop(buffer);
        self.reveal_line(line);
        result
    }

    /// The buffer's observers must not fire for these: an autosave or a vault
    /// reload subscribes to real edits, not caret moves.
    fn update_state(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState)) {
        f(&mut self.state);
        cx.notify();
    }

    /// [`Self::update_state`] for movements that need the buffer to resolve.
    fn move_cursor(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &Buffer)) {
        let buffer = self.buffer.read(cx);
        f(&mut self.state, buffer);
        // Before `cx.notify()`: `Entity::read` borrows `cx`, not the entity.
        let line = buffer.line_for_offset(self.state.cursor());
        cx.notify();
        self.reveal_line(line);
    }

    /// Called from both gates, not the action handlers, which would forget it.
    /// Must be written here: from `TextElement::prepaint` it lands a frame late
    /// (`gpui/elements/div.rs:2390-2392`, then `:2027`). The unclamped write is
    /// deliberate — see [`scroll::reveal`].
    fn reveal_line(&mut self, line: usize) {
        let (Some(text_top), Some(layout)) = (self.last_text_top, self.last_layout.as_ref()) else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        // Nothing prepainted yet: bounds stay `default()` until the first
        // `clamp_scroll_position` (`gpui/elements/div.rs:2484`).
        if viewport.size.height <= px(0.) {
            return;
        }

        let top = text_top + layout.line_height() * line;
        let line = top..top + layout.line_height();
        let viewport = viewport.top()..viewport.bottom();

        if let Some(offset) = scroll::reveal(line, viewport, self.scroll_handle.offset().y) {
            self.scroll_handle
                .set_offset(point(self.scroll_handle.offset().x, offset));
        }
    }

    /// The only movements needing font metrics, and the only ones that set a
    /// [`Goal`] — everything else names `Goal::None`.
    fn move_vertically(&mut self, cx: &mut Context<Self>, direction: Vertical, extend: bool) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let cursor = self.cursor();
        let line = layout.line_for_offset(cursor);

        // Reused verbatim: it re-widens a run of downs after a short line.
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

        // Drops the goal on purpose: the next vertical move should start from
        // the column this landed in, not the one it was aiming for.
        let movement = match direction {
            Vertical::Up => LogicalMovement::DocStart,
            Vertical::Down => LogicalMovement::DocEnd,
        };
        self.move_cursor(cx, |state, buffer| state.move_(buffer, movement, extend));
    }

    fn cursor(&self) -> ByteOffset {
        self.state.cursor()
    }

    /// Unclipped; both callers pass the result to `move_to`, which clips.
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
    // Underscore grouping (0x001e_1e1e) obscures the RGB byte boundaries.
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
