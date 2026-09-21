//! Action declarations and their handlers.
//!
//! Each handler is a thin delegation to `inkra-editor` through one of
//! `EditorView`'s gates — except [`EditorView::copy`], which goes through none
//! of them because it reads. Keeping them thin is the point: the logic they
//! call is tested without a window, and they are covered end to end by the
//! dispatch tests, which drive real keystrokes rather than calling these
//! functions directly.

use std::time::Instant;

use gpui::{ClipboardItem, Context, Window, actions};
use inkra_editor::{Copied, LogicalMovement};

use super::{EditorView, Vertical};

actions!(
    editor,
    [
        Backspace,
        Copy,
        Cut,
        Delete,
        Left,
        Paste,
        Right,
        SelectLeft,
        SelectRight,
        Up,
        Down,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        DocStart,
        DocEnd,
        Newline,
        Undo,
        Redo,
    ]
);

/// The metadata a whole-line copy carries beside its text.
///
/// gpui writes it to a macOS pasteboard type it hard-codes as `zed-metadata`
/// (`gpui_macos/src/pasteboard.rs:48`), which Zed itself fills with JSON. So
/// the read side matches this exact string and treats anything else — Zed's
/// `[{"len":…}]` included — as an ordinary span. Guessing at a foreign format
/// would be worse than ignoring it.
const WHOLE_LINE: &str = "line";

fn to_clipboard(copied: Copied) -> ClipboardItem {
    if copied.whole_line {
        ClipboardItem::new_string_with_metadata(copied.text, WHOLE_LINE.to_owned())
    } else {
        ClipboardItem::new_string(copied.text)
    }
}

/// `None` when there is nothing to paste — no string entry, or an empty one.
/// Both must stop before the buffer is touched: `EditorState::paste` seals the
/// transaction around its edit, and sealing around an edit that never lands
/// would split the surrounding typing run into two undo steps.
fn from_clipboard(item: &ClipboardItem) -> Option<Copied> {
    let text = inkra_vault::normalize_line_endings(&item.text()?);
    if text.is_empty() {
        return None;
    }
    // The trailing newline is `Copied`'s invariant, not the marker's. A foreign
    // writer that happens to use our token cannot make us weld two lines.
    let whole_line =
        item.metadata().is_some_and(|marker| marker == WHOLE_LINE) && text.ends_with('\n');
    Some(Copied { text, whole_line })
}

impl EditorView {
    pub(super) fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.backspace(buffer, now));
    }

    /// The one action that takes no gate. A copy changes neither the text nor
    /// the view, so it owes no repaint; `edit_buffer`'s `as_mut` notifies
    /// unconditionally (`gpui/src/app.rs:3280-3286`), which would schedule an
    /// autosave for a keystroke that changed nothing.
    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.state.copy(self.buffer.read(cx));
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let copied = self.edit_buffer(cx, |state, buffer| state.cut(buffer, now));
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(copied) = cx.read_from_clipboard().as_ref().and_then(from_clipboard) else {
            return;
        };
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.paste(buffer, now, &copied));
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

    pub(super) fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(cx, Vertical::Up, false);
    }

    pub(super) fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(cx, Vertical::Down, false);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(cx, Vertical::Up, true);
    }

    pub(super) fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(cx, Vertical::Down, true);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, inkra_editor::EditorState::select_all);
    }

    pub(super) fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::LineStart, false);
        });
    }

    pub(super) fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::LineEnd, false);
        });
    }

    pub(super) fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocStart, false);
        });
    }

    pub(super) fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocEnd, false);
        });
    }

    pub(super) fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.insert(buffer, now, "\n"));
    }

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(cx, inkra_editor::EditorState::undo);
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(cx, inkra_editor::EditorState::redo);
    }
}
