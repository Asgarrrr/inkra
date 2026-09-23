//! The editor view: the gpui half of editing. Everything here needs a `Window`,
//! font metrics, or an entity handle; anything else belongs in `inkra-editor`.

pub(crate) mod actions;
pub(crate) mod element;
mod input;
mod scroll;
#[cfg(test)]
mod tests;

use gpui::{
    App, Bounds, Context, CursorStyle, Entity, FocusHandle, Focusable, InteractiveElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, div, point, px,
};
use inkra_editor::{EditorState, Goal, LogicalMovement, Selection};
use inkra_text::{Buffer, ByteOffset};

use crate::theme;
use element::{DocumentLayout, TextElement};

/// Local: up and down are not resolvable without font metrics.
#[derive(Debug, Clone, Copy)]
pub(super) enum Vertical {
    Up,
    Down,
}

/// Local, like [`Vertical`]: the visual row's ends need the layout.
#[derive(Debug, Clone, Copy)]
pub(super) enum RowEdge {
    Start,
    End,
}

pub(crate) struct EditorView {
    buffer: Entity<Buffer>,
    state: EditorState,
    focus_handle: FocusHandle,
    is_selecting: bool,
    /// Written in `paint` and by `edit_buffer`, so it describes the current
    /// text. Every index into it still clamps; see `DocumentLayout`.
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

        // Cleared, not kept: `reveal_cursor` would scroll the new note by the
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

    /// Where the text column was painted. `#[cfg(test)]`, like `buffer`: a
    /// shell test needs it to see the column inside its real parent.
    #[cfg(test)]
    pub(crate) fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_bounds
    }

    /// The single path that mutates the buffer. `as_mut`, not `update`: it
    /// notifies on drop (`gpui/src/app.rs:3280-3286`), and `Buffer` is gpui-free by
    /// rule, so nothing else can.
    ///
    /// An action that mutates neither the buffer nor `EditorState` needs none
    /// of the gates and must not take one: `copy` reads with `buffer.read` so
    /// that `⌘C` cannot notify, and so cannot mark a clean note dirty.
    ///
    /// Reshapes before revealing: the edit may have moved every line after it,
    /// and the stored layout describes the text before the edit. A caret move
    /// changes no text, which is why `move_cursor` does not.
    fn edit_buffer<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R,
    ) -> R {
        let mut buffer = self.buffer.as_mut(cx);
        let result = f(&mut self.state, &mut buffer);
        // Load-bearing: the lease would outlive the reshape below, which reads
        // the buffer, and a second lease panics at runtime
        // (`gpui/src/app/entity_map.rs:231`).
        drop(buffer);
        self.reshape(window, cx);
        self.reveal_cursor();
        result
    }

    /// Shapes the current text the way the last frame did. Nothing painted
    /// yet means nothing to copy the style from, and nothing to reveal against.
    fn reshape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.last_layout.as_ref() else {
            return;
        };
        let (style, wrap_width) = (last.style().clone(), last.wrap_width());
        let marked = self.state.marked_range();
        self.last_layout = Some(element::shape_document(
            self.buffer.read(cx),
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

    /// [`Self::update_state`] for movements that need the buffer to resolve.
    fn move_cursor(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &Buffer)) {
        f(&mut self.state, self.buffer.read(cx));
        cx.notify();
        self.reveal_cursor();
    }

    /// Called from both gates, not the action handlers, which would forget it.
    /// Must be written here: from `TextElement::prepaint` it lands a frame late
    /// (`gpui/elements/div.rs:2390-2392`, then `:2027`). The unclamped write is
    /// deliberate — see [`scroll::reveal`].
    fn reveal_cursor(&mut self) {
        let (Some(text_top), Some(layout)) = (self.last_text_top, self.last_layout.as_ref()) else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        // Nothing prepainted yet: bounds stay `default()` until the first
        // `clamp_scroll_position` (`gpui/elements/div.rs:2484`).
        if viewport.size.height <= px(0.) {
            return;
        }

        let top = text_top + layout.top_for_row(layout.row_for_offset(self.cursor()));
        let row = top..top + layout.line_height();
        let viewport = viewport.top()..viewport.bottom();

        if let Some(offset) = scroll::reveal(row, viewport, self.scroll_handle.offset().y) {
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
        let row = layout.row_for_offset(cursor);

        // Reused verbatim: it re-widens a run of downs after a short row. An x
        // from the row's start, so it means the same on every row.
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

    /// Home and End: the ends of the caret's visual row. A second press stays
    /// put — macOS has no escalation to the logical line; `ctrl-a`/`ctrl-e` go
    /// there instead. Nothing painted yet means no rows, and, like up and down,
    /// no move.
    fn move_to_row_edge(&mut self, cx: &mut Context<Self>, edge: RowEdge) {
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let row = layout.row_bounds(layout.row_for_offset(self.cursor()));
        let target = match edge {
            RowEdge::Start => *row.start(),
            RowEdge::End => *row.end(),
        };
        self.move_cursor(cx, |state, buffer| state.move_to(buffer, target, false));
    }

    /// `pub(crate)` for `shell.rs`'s tests, which assert the caret did *not*
    /// move when a key was claimed elsewhere. Used inside this module too, so
    /// it is not dead in a release build.
    pub(crate) fn cursor(&self) -> ByteOffset {
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
