//! The editor view: the gpui half of editing.
//!
//! Everything here needs a `Window`, font metrics, or an entity handle.
//! Anything that does not belongs in `inkra-editor`.

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
    /// Where the text would start at zero scroll, as of the last paint.
    ///
    /// Deliberately stored rather than derived as `last_bounds.origin.y -
    /// scroll_handle.offset().y` on demand: `last_bounds` already has the
    /// offset of the frame that produced it baked in, while `offset()` is a
    /// live cell. Two cursor moves inside one update — or a wheel scroll — put
    /// the two out of step, and subtracting a newer offset from an older
    /// bounds counts the scroll twice.
    last_text_top: Option<Pixels>,
    /// gpui applies the scroll offset to this element's children in `prepaint`
    /// (`gpui/elements/div.rs:2027`), so `last_bounds` already arrives scrolled
    /// and every hit-test downstream stays correct without subtracting it.
    /// `last_text_top` is the one place that does subtract it, to undo exactly
    /// this — see its own comment.
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
            last_text_top: None,
            scroll_handle: ScrollHandle::new(),
            _buffer_changed: buffer_changed,
        }
    }

    /// The third gate: replaces the buffer this view edits.
    ///
    /// `edit_buffer` hands out `&mut Buffer` and `move_cursor` a `&Buffer`;
    /// neither can rebind `self.buffer`, and going through them would push the
    /// new note's text onto the *previous* note's undo stack — `⌘Z` would then
    /// restore one file into another.
    ///
    /// Every field reset below is a defect if skipped, so they are exhaustive
    /// rather than minimal.
    // `used_underscore_binding`: the underscore says the field is held only for
    // its `Drop`, and reassigning it is exactly this method's point. Renaming it
    // does not help — rustc exempts `_`-prefixed fields from "never read", so a
    // plain name trades this lint for that one.
    #[expect(
        clippy::used_underscore_binding,
        reason = "resubscribing is the point of the gate"
    )]
    pub(crate) fn open_buffer(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        // Rebuilt, not kept: the old `Subscription` unsubscribes when it drops,
        // and keeping it would observe the closed note forever.
        self._buffer_changed = cx.observe(&buffer, |_, _, cx| cx.notify());
        self.buffer = buffer;

        // Offset 0, unlike `new`, which seats the caret at `end`. Opening a
        // note starts at the top; resuming the session does not.
        self.state = EditorState::new(Selection::at(ByteOffset(0)));
        self.is_selecting = false;

        // `last_layout` describes the previous note's text. `reveal_line`
        // early-returns on `None`, so a stale non-`None` layout would scroll the
        // new note by the old note's geometry.
        self.last_layout = None;
        self.last_bounds = None;
        self.last_text_top = None;
        self.scroll_handle = ScrollHandle::new();

        cx.notify();
    }

    /// The buffer this view currently edits.
    ///
    /// `#[cfg(test)]`: inside `editor/` the field is reachable directly, and no
    /// production caller wants it — `open_buffer` sets it, nothing reads it
    /// back. `sidebar.rs` is another module, so its test needs this.
    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
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
        let result = f(&mut self.state, &mut buffer);
        let line = buffer.line_for_offset(self.state.cursor());
        // Explicit, though the borrow checker does not need it: otherwise the
        // lease outlives `reveal_line`, and a second lease panics at runtime
        // rather than failing to compile (`gpui/app/entity_map.rs:231`).
        drop(buffer);
        self.reveal_line(line);
        result
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
        // Before `cx.notify()`, not after: `Entity::read` ties the borrow to
        // `cx` rather than to the entity, so a last use of `buffer` past the
        // notify holds an immutable borrow of `cx` across a mutable one.
        let line = buffer.line_for_offset(self.state.cursor());
        cx.notify();
        self.reveal_line(line);
    }

    /// Scrolls `line` into view, if it is not there already.
    ///
    /// Called from both gates rather than from the action handlers, for the
    /// reason the gates exist at all: a reveal added by hand at each call site
    /// is one the next action forgets. Clicks and drags come through here too
    /// and cost nothing — they resolve a point the user just pointed at, so the
    /// line is on screen by construction.
    ///
    /// Applied now rather than deferred to prepaint, because the containing
    /// `div` reads and clamps the offset *before* it prepaints its children
    /// (`gpui/elements/div.rs:2390-2392`, then `:2027`). Writing it here lands
    /// in this frame; writing it from `TextElement::prepaint` would land in the
    /// next one. The unclamped write is deliberate — see [`scroll::reveal`].
    fn reveal_line(&mut self, line: usize) {
        let (Some(text_top), Some(layout)) = (self.last_text_top, self.last_layout.as_ref()) else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        // Nothing has been prepainted yet, so there is no viewport to reveal
        // into: `ScrollHandle::bounds` is `Bounds::default()` until the first
        // `clamp_scroll_position` writes it (`gpui/elements/div.rs:2484`).
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
