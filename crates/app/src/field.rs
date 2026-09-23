//! One line of editable text, for naming things. Speaks the platform input
//! protocol via `EntityInputHandler`, not key events — a dead key or IME
//! delivers marked text, which a key-down reader would drop.

use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, InteractiveElement,
    IntoElement, LayoutId, Pixels, Render, Style, Styled, UTF16Selection, Window, actions, fill,
    point, prelude::*, px, relative, size,
};
use inkra_editor::{EditorState, LogicalMovement, Selection};
use inkra_text::{Buffer, ByteOffset};

use crate::editor::layout::{DocumentLayout, shape_document};
use crate::theme;

actions!(field, [Confirm, Cancel]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirmed(pub(crate) String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;

impl EventEmitter<Confirmed> for NameField {}
impl EventEmitter<Cancelled> for NameField {}

pub(crate) struct NameField {
    buffer: Entity<Buffer>,
    state: EditorState,
    focus_handle: FocusHandle,
    last_layout: Option<DocumentLayout>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl NameField {
    pub(crate) fn new(initial: &str, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|_| Buffer::new(initial));
        let end = buffer.read(cx).len();
        let mut selection = Selection::at(ByteOffset(0));
        selection.set_head(end, inkra_editor::Goal::None);
        Self {
            buffer,
            state: EditorState::new(selection),
            focus_handle: cx.focus_handle(),
            last_layout: None,
            last_bounds: None,
        }
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.buffer.read(cx).text().to_owned()
    }

    /// The only path that mutates the text; mirrors `EditorView::edit_buffer`.
    /// The lease notifies on drop, so a mutation outside this loses its repaint.
    fn edit_buffer(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer),
    ) {
        let buffer = self.buffer.clone();
        buffer.update(cx, |buffer, cx| {
            f(&mut self.state, buffer);
            cx.notify();
        });
        cx.notify();
    }

    fn move_cursor(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &Buffer)) {
        f(&mut self.state, self.buffer.read(cx));
        cx.notify();
    }

    fn movement(&mut self, movement: LogicalMovement, extend: bool, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, movement, extend);
        });
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Confirmed(self.text(cx)));
    }

    fn cancel(_: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(Cancelled);
    }
}

/// The handlers, one per key the field claims — repeats the editor's set
/// since every editing binding names `EditorView`'s context (see CLAUDE.md).
impl NameField {
    fn backspace(
        &mut self,
        _: &crate::editor::actions::Backspace,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.backspace(buffer, now));
    }

    fn delete(
        &mut self,
        _: &crate::editor::actions::Delete,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.delete(buffer, now));
    }

    fn left(&mut self, _: &crate::editor::actions::Left, _: &mut Window, cx: &mut Context<Self>) {
        self.movement(LogicalMovement::Left, false, cx);
    }

    fn right(&mut self, _: &crate::editor::actions::Right, _: &mut Window, cx: &mut Context<Self>) {
        self.movement(LogicalMovement::Right, false, cx);
    }

    fn select_left(
        &mut self,
        _: &crate::editor::actions::SelectLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.movement(LogicalMovement::Left, true, cx);
    }

    fn select_right(
        &mut self,
        _: &crate::editor::actions::SelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.movement(LogicalMovement::Right, true, cx);
    }

    fn home(&mut self, _: &crate::editor::actions::Home, _: &mut Window, cx: &mut Context<Self>) {
        self.movement(LogicalMovement::LineStart, false, cx);
    }

    fn end(&mut self, _: &crate::editor::actions::End, _: &mut Window, cx: &mut Context<Self>) {
        self.movement(LogicalMovement::LineEnd, false, cx);
    }

    fn select_all(
        &mut self,
        _: &crate::editor::actions::SelectAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursor(cx, EditorState::select_all);
    }
}

impl Focusable for NameField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NameField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .key_context("NameField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, action, window, cx| Self::cancel(action, window, cx)))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_all))
            .size_full()
            .text_color(theme::sidebar_text())
            .child(FieldElement { field: cx.entity() })
    }
}

/// A newline can arrive via paste or IME rather than a key press; flattened
/// to a space since the field is one line, and 1:1 UTF-16 keeps ranges valid.
fn flatten_newlines(text: &str) -> String {
    text.replace('\n', " ")
}

impl EntityInputHandler for NameField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.buffer.read(cx);
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
        let buffer = self.buffer.read(cx);
        let selection = self.state.selection();
        Some(UTF16Selection {
            range: buffer.to_utf16_range(&selection.range()),
            reversed: selection.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.state.marked_range()?;
        Some(self.buffer.read(cx).to_utf16_range(&range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.state.set_marked_range(None);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let flattened = flatten_newlines(new_text);
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| {
            state.replace_in_range(buffer, now, range_utf16.as_ref(), &flattened);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let flattened = flatten_newlines(new_text);
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| {
            state.mark_and_replace_in_range(
                buffer,
                now,
                range_utf16.as_ref(),
                &flattened,
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
        let layout = self.last_layout.as_ref()?;
        let range = self.buffer.read(cx).from_utf16_range(&range_utf16);
        Some(Bounds::from_corners(
            element_bounds.origin + layout.position_for_offset(range.start),
            element_bounds.origin + point(layout.x_for_offset(range.end), layout.line_height()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let offset = layout.offset_on_row(0, position.x - bounds.origin.x);
        let buffer = self.buffer.read(cx);
        Some(buffer.to_utf16(buffer.clip(offset)).0)
    }
}

struct FieldElement {
    field: Entity<NameField>,
}

struct FieldPrepaint {
    layout: Option<DocumentLayout>,
    selection: Option<gpui::PaintQuad>,
    cursor: Option<gpui::PaintQuad>,
}

impl IntoElement for FieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

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
        let field = self.field.read(cx);
        let buffer = field.buffer.read(cx);
        let range = field.state.selection().range();
        let marked = field.state.marked_range();

        let style = window.text_style();
        let layout = shape_document(buffer, marked.as_ref(), &style, None, window);
        let line_height = layout.line_height();

        let (selection, cursor) = if range.is_empty() {
            let at = layout.position_for_offset(range.start);
            let quad = fill(
                Bounds::new(bounds.origin + at, size(px(2.), line_height)),
                theme::cursor(),
            );
            (None, Some(quad))
        } else {
            let quad = fill(
                Bounds::from_corners(
                    bounds.origin + point(layout.x_for_offset(range.start), px(0.)),
                    bounds.origin + point(layout.x_for_offset(range.end), line_height),
                ),
                theme::selection(),
            );
            (Some(quad), None)
        };

        FieldPrepaint {
            layout: Some(layout),
            selection,
            cursor,
        }
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
        let focus_handle = self.field.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );

        if let Some(quad) = prepaint.selection.take() {
            window.paint_quad(quad);
        }

        let Some(layout) = prepaint.layout.take() else {
            return;
        };
        layout.paint(bounds.origin, window, cx);

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.field.update(cx, |field, _| {
            field.last_layout = Some(layout);
            field.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
