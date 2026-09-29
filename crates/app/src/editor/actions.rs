use std::time::Instant;

use gpui::{ClipboardItem, Context, Window, actions};
use inkra_editor::{Copied, LogicalMovement};

use super::motion::Vertical;
use super::{EditorView, LinkFollowed};
use crate::text_input::InputHost;

actions!(
    editor,
    [
        Copy, Cut, Paste, Up, Down, SelectUp, SelectDown, DocStart, DocEnd, Newline, Undo, Redo,
        FollowLink,
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
    /// The one action taking no gate: `edit_buffer`'s `as_mut` notifies
    /// unconditionally, which would schedule an autosave for a keystroke that changed nothing.
    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.input.state().copy(self.input.buffer().read(cx));
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

    pub(super) fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(window, cx, Vertical::Up, false);
    }

    pub(super) fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(window, cx, Vertical::Down, false);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(window, cx, Vertical::Up, true);
    }

    pub(super) fn select_down(
        &mut self,
        _: &SelectDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_vertically(window, cx, Vertical::Down, true);
    }

    pub(super) fn doc_start(&mut self, _: &DocStart, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(window, cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocStart, false);
        });
    }

    pub(super) fn doc_end(&mut self, _: &DocEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(window, cx, |state, buffer| {
            state.move_(buffer, LogicalMovement::DocEnd, false);
        });
    }

    pub(super) fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| state.insert(buffer, now, "\n"));
    }

    /// Follows at the selection's head; the view never resolves, the shell does.
    pub(super) fn follow_link(&mut self, _: &FollowLink, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.buffer().read(cx).text();
        if let Some(link) = inkra_markdown::link_at(text, self.cursor()) {
            cx.emit(LinkFollowed(link));
        }
    }

    pub(super) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::undo);
    }

    pub(super) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::redo);
    }
}
