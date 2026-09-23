use std::time::Instant;

use gpui::{ClipboardItem, Context, Window, actions};
use inkra_editor::{Copied, LogicalMovement};

use super::{EditorView, RowEdge, Vertical};

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
        LineStart,
        LineEnd,
        DocStart,
        DocEnd,
        Newline,
        Undo,
        Redo,
    ]
);

/// The metadata a whole-line copy carries, matching gpui's pasteboard type
/// (`zed-metadata`); anything else, Zed's own JSON included, reads as an ordinary span.
const WHOLE_LINE: &str = "line";

fn to_clipboard(copied: Copied) -> ClipboardItem {
    if copied.whole_line {
        ClipboardItem::new_string_with_metadata(copied.text, WHOLE_LINE.to_owned())
    } else {
        ClipboardItem::new_string(copied.text)
    }
}

/// `None` when there's nothing to paste. Both must stop before the buffer is
/// touched: sealing a transaction around an edit that never lands splits the typing run.
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
    pub(super) fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| state.backspace(buffer, now));
    }

    /// The one action taking no gate: `edit_buffer`'s `as_mut` notifies
    /// unconditionally, which would schedule an autosave for a keystroke that changed nothing.
    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.state.copy(self.buffer.read(cx));
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let copied = self.edit_buffer(window, cx, |state, buffer| state.cut(buffer, now));
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(copied) = cx.read_from_clipboard().as_ref().and_then(from_clipboard) else {
            return;
        };
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| {
            state.paste(buffer, now, &copied);
        });
    }

    pub(super) fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| state.delete(buffer, now));
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
        self.move_to_row_edge(cx, RowEdge::Start);
    }

    pub(super) fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to_row_edge(cx, RowEdge::End);
    }

    pub(super) fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::LineStart, false);
        });
    }

    pub(super) fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| state.insert(buffer, now, "\n"));
    }

    pub(super) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::undo);
    }

    pub(super) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::redo);
    }
}
