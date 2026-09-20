//! Action declarations and their handlers.
//!
//! Each handler is a thin delegation to `inkra-editor` through one of
//! `EditorView`'s two gates. Keeping them thin is the point: the logic they
//! call is tested without a window, and they are covered end to end by the
//! dispatch tests, which drive real keystrokes rather than calling these
//! functions directly.

use std::time::Instant;

use gpui::{Context, Window, actions};
use inkra_editor::LogicalMovement;

use super::EditorView;

actions!(
    editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Undo,
        Redo,
    ]
);

impl EditorView {
    pub(super) fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.backspace(buffer, now));
    }

    pub(super) fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.delete(buffer, now));
    }

    pub(super) fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::Left, false);
        });
    }

    pub(super) fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::Right, false);
        });
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::Left, true);
        });
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::Right, true);
        });
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, inkra_editor::EditorState::select_all);
    }

    // Home and End map to the document ends while there is one line. Multi-line
    // turns them into line ends, which is a `LogicalMovement` variant that does
    // not exist yet because nothing can answer it: `Buffer` gains a line index
    // in that same change.
    pub(super) fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocStart, false);
        });
    }

    pub(super) fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocEnd, false);
        });
    }

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(cx, inkra_editor::EditorState::undo);
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(cx, inkra_editor::EditorState::redo);
    }
}
