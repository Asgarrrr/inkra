//! One editable text for every view that has one: the buffer, the selection, the
//! platform input protocol and the gates. `EditorView` and `NameField` host it
//! (text-input spec). Speaks the protocol, not key events: a dead key or IME
//! delivers marked text, which a key-down reader would drop.

pub(crate) mod actions;
mod adapter;
mod paint;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

use std::borrow::Cow;
use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, KeyContext, Pixels, Point, Render, UTF16Selection,
    Window,
};
use inkra_editor::{EditorState, Selection};
use inkra_text::{Buffer, ByteOffset, Diff};

use crate::editor::layout::DocumentLayout;

pub(crate) use actions::on_actions;
pub(crate) use adapter::InputAdapter;
pub(crate) use paint::{Prepainted, Quads, paint};

/// The key context every host adds: the shared bindings name it, so they match in each host.
pub(crate) const CONTEXT: &str = "TextInput";
/// Each host's own key context, for the bindings it adds on top of the shared ones.
pub(crate) const EDITOR_CONTEXT: &str = "EditorView";
pub(crate) const FIELD_CONTEXT: &str = "NameField";
/// Every host, so a check over hosts cannot skip one.
#[cfg(test)]
pub(crate) const HOST_CONTEXTS: [&str; 2] = [EDITOR_CONTEXT, FIELD_CONTEXT];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    SingleLine,
    Multiline,
}

impl Mode {
    /// Text from the platform. A newline can arrive by paste or IME, not only by
    /// a key; one line turns it into a space, one UTF-16 unit for one, so ranges stay valid.
    fn admit(self, text: &str) -> Cow<'_, str> {
        match self {
            Self::SingleLine if text.contains('\n') => Cow::Owned(text.replace('\n', " ")),
            Self::SingleLine | Self::Multiline => Cow::Borrowed(text),
        }
    }
}

pub(crate) struct TextInput {
    buffer: Entity<Buffer>,
    state: EditorState,
    mode: Mode,
    focus_handle: FocusHandle,
    /// Shared with the editor's measure, which reuses it when its inputs still hold.
    last_layout: Option<Rc<DocumentLayout>>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl TextInput {
    pub(crate) fn new(
        buffer: Entity<Buffer>,
        selection: Selection,
        mode: Mode,
        cx: &mut App,
    ) -> Self {
        Self {
            buffer,
            state: EditorState::new(selection),
            mode,
            focus_handle: cx.focus_handle(),
            last_layout: None,
            last_bounds: None,
        }
    }

    /// Only `EditorView::open_buffer` calls this: the selection and the layout
    /// describe the old buffer, so they go with it and the caret starts at 0.
    pub(crate) fn rebind(&mut self, buffer: Entity<Buffer>) {
        self.buffer = buffer;
        self.state = EditorState::new(Selection::at(ByteOffset(0)));
        self.last_layout = None;
        self.last_bounds = None;
    }

    pub(crate) fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
    }

    pub(crate) fn state(&self) -> &EditorState {
        &self.state
    }

    /// Past the gates, for a test that sets up a state no key reaches.
    #[cfg(test)]
    pub(crate) fn state_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }

    pub(crate) fn cursor(&self) -> ByteOffset {
        self.state.cursor()
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(crate) fn last_layout(&self) -> Option<&Rc<DocumentLayout>> {
        self.last_layout.as_ref()
    }

    /// Already scrolled when the host scrolls: gpui translates the element's bounds.
    #[cfg(test)]
    pub(crate) fn last_bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_bounds
    }

    /// The layout and a window point in its coordinates. `last_bounds` is already
    /// scrolled, so no caller subtracts the offset.
    pub(crate) fn hit_test(
        &self,
        position: Point<Pixels>,
    ) -> Option<(&DocumentLayout, Point<Pixels>)> {
        let (bounds, layout) = (self.last_bounds?, self.last_layout.as_ref()?);
        Some((layout, position - bounds.origin))
    }

    /// The offset nearest a window point; `None` before the first draw.
    pub(crate) fn offset_at(&self, position: Point<Pixels>) -> Option<ByteOffset> {
        let (layout, local) = self.hit_test(position)?;
        Some(layout.offset_for_position(local))
    }

    /// A reshape outside a draw; the draw itself stores through `paint`.
    pub(crate) fn set_layout(&mut self, layout: Rc<DocumentLayout>) {
        self.last_layout = Some(layout);
    }

    /// A change from disk: the selection and the composition follow their text.
    pub(crate) fn map_through(&mut self, diff: &Diff) {
        self.state.map_through(diff);
    }

    /// `"TextInput <host>"`: the shared bindings match every host, a host's own only it.
    pub(crate) fn key_context(host: &'static str) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(CONTEXT);
        context.add(host);
        context
    }

    fn store(&mut self, layout: Rc<DocumentLayout>, bounds: Bounds<Pixels>) {
        self.last_layout = Some(layout);
        self.last_bounds = Some(bounds);
    }
}

/// The view that owns a `TextInput`. Two implementers: `EditorView`, `NameField`.
pub(crate) trait InputHost: Render + Sized {
    fn input(&self) -> &TextInput;
    fn input_mut(&mut self) -> &mut TextInput;
    /// After every `edit_buffer`; `edited` is false when the text did not change.
    fn after_edit(&mut self, edited: bool, window: &mut Window, cx: &mut Context<Self>);
    /// After every `move_cursor`; `old` is the selection before the move.
    fn after_move(&mut self, old: Range<ByteOffset>, window: &mut Window, cx: &mut Context<Self>);

    /// The only path that mutates the text. The lease notifies the buffer's
    /// observers on drop; it is dropped before `after_edit`, which may read the buffer.
    fn edit_buffer<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R,
    ) -> R {
        let input = self.input_mut();
        let before = input.buffer.read(cx).version();
        let mut buffer = input.buffer.as_mut(cx);
        let result = f(&mut input.state, &mut buffer);
        // A second lease panics, so this one ends before anything reads the buffer.
        drop(buffer);
        let edited = input.buffer.read(cx).version() != before;
        cx.notify();
        self.after_edit(edited, window, cx);
        result
    }

    /// Selection moves. Never leases: an autosave or a reload observes the
    /// buffer, and a caret move is not an edit.
    fn move_cursor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &Buffer),
    ) {
        let input = self.input_mut();
        let old = input.state.selection().range();
        f(&mut input.state, input.buffer.read(cx));
        cx.notify();
        self.after_move(old, window, cx);
    }

    // The platform input protocol, as `EntityInputHandler` names it; `InputAdapter` forwards to it.

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.input().buffer.read(cx);
        let range = buffer.from_utf16_range(&range_utf16);
        actual_range.replace(buffer.to_utf16_range(&range));
        Some(buffer.text()[range.start.0..range.end.0].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let input = self.input();
        let selection = input.state.selection();
        Some(UTF16Selection {
            range: input.buffer.read(cx).to_utf16_range(&selection.range()),
            reversed: selection.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let input = self.input();
        let range = input.state.marked_range()?;
        Some(input.buffer.read(cx).to_utf16_range(&range))
    }

    /// Not an edit: no hook runs. It still repaints, since the underline goes.
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.input_mut().state.set_marked_range(None);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.input().mode.admit(new_text);
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| {
            state.replace_in_range(buffer, now, range_utf16.as_ref(), &text);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.input().mode.admit(new_text);
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| {
            state.mark_and_replace_in_range(
                buffer,
                now,
                range_utf16.as_ref(),
                &text,
                new_selected_range_utf16,
            );
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let input = self.input();
        let layout = input.last_layout.as_ref()?;
        let range = input.buffer.read(cx).from_utf16_range(&range_utf16);
        // `range_bounds` is element-relative and already clamped to the row
        // holding `range.start`; the platform wants window coordinates.
        let local = layout.range_bounds(&range);
        Some(Bounds::new(
            element_bounds.origin + local.origin,
            local.size,
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let input = self.input();
        let offset = input.offset_at(position)?;
        let buffer = input.buffer.read(cx);
        Some(buffer.to_utf16(buffer.clip(offset)).0)
    }
}
