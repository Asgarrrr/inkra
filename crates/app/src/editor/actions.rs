use std::time::Instant;

use gpui::{ClipboardItem, Context, Div, InteractiveElement, Window, actions};
use inkra_editor::{Copied, EditorState, LogicalMovement};
use inkra_text::Diff;

use super::motion::Vertical;
use super::parses;
use super::{EditorView, LinkFollowed};
use crate::open::Open;
use crate::text_input::InputHost;

actions!(
    editor,
    [
        Copy,
        Cut,
        Paste,
        Up,
        Down,
        SelectUp,
        SelectDown,
        DocStart,
        DocEnd,
        Newline,
        Undo,
        Redo,
        FollowLink,
        FollowLinkInNewTab,
        FollowLinkInSplit,
        ToggleTask,
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

/// The editor's own actions; `text_input::on_actions` binds those it shares with the name field.
pub(super) fn on_actions(div: Div, cx: &mut Context<EditorView>) -> Div {
    div.on_action(cx.listener(EditorView::copy))
        .on_action(cx.listener(EditorView::cut))
        .on_action(cx.listener(EditorView::paste))
        .on_action(cx.listener(EditorView::up))
        .on_action(cx.listener(EditorView::down))
        .on_action(cx.listener(EditorView::select_up))
        .on_action(cx.listener(EditorView::select_down))
        .on_action(cx.listener(EditorView::doc_start))
        .on_action(cx.listener(EditorView::doc_end))
        .on_action(cx.listener(EditorView::newline))
        .on_action(cx.listener(EditorView::undo))
        .on_action(cx.listener(EditorView::redo))
        .on_action(cx.listener(EditorView::follow_link))
        .on_action(cx.listener(EditorView::follow_link_in_new_tab))
        .on_action(cx.listener(EditorView::follow_link_in_split))
        .on_action(cx.listener(EditorView::toggle_task))
}

impl EditorView {
    /// The one action taking no gate: `edit_buffer`'s `as_mut` notifies
    /// unconditionally, which would schedule an autosave for a keystroke that changed nothing.
    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.input.state().copy(self.input.buffer().read(cx));
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let copied = self.edit_buffer(window, cx, EditorState::cut);
        cx.write_to_clipboard(to_clipboard(copied));
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(copied) = cx.read_from_clipboard().as_ref().and_then(from_clipboard) else {
            return;
        };
        self.edit_buffer(window, cx, |state, buffer| {
            state.paste(buffer, &copied);
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

    pub(super) fn follow_link(&mut self, _: &FollowLink, _: &mut Window, cx: &mut Context<Self>) {
        self.follow_at_cursor(Open::Here, cx);
    }

    pub(super) fn follow_link_in_new_tab(
        &mut self,
        _: &FollowLinkInNewTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.follow_at_cursor(Open::NewTab, cx);
    }

    pub(super) fn follow_link_in_split(
        &mut self,
        _: &FollowLinkInSplit,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.follow_at_cursor(Open::SplitRight, cx);
    }

    /// Follows at the selection's head; the view never resolves, the shell does.
    fn follow_at_cursor(&mut self, open: Open, cx: &mut Context<Self>) {
        let text = self.input.buffer().read(cx).text();
        if let Some(link) = inkra_markdown::link_at(text, self.cursor()) {
            cx.emit(LinkFollowed { link, open });
        }
    }

    /// A skipped line takes no lease: `as_mut` notifies, which would schedule an autosave.
    fn toggle_task(&mut self, _: &ToggleTask, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.cursor();
        let markdown = parses::parsed(self.input.buffer(), cx);
        let buffer = self.input.buffer().read(cx);
        let hunks = inkra_markdown::cycle_tasks(buffer.text(), &markdown, caret..caret);
        if !hunks.is_empty() {
            let cycle = Diff::new(buffer.version(), hunks);
            self.edit_buffer(window, cx, |state, buffer| state.apply_diff(buffer, &cycle));
        }
    }

    pub(super) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::undo);
    }

    pub(super) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(window, cx, inkra_editor::EditorState::redo);
    }
}
