//! The list commands' handlers: each asks `inkra-markdown` for a `ListEdit` on the cached parse
//! and applies it as one step (list editing spec, D9).

use std::time::Instant;

use gpui::{Context, Window};
use inkra_text::Diff;

use super::Newline;
use crate::editor::EditorView;
use crate::text_input::InputHost;

impl EditorView {
    /// A selection, marked text or no list rule takes the plain `\n`; a stale parse edits nothing,
    /// since its offsets may not fit the text.
    pub(in crate::editor) fn newline(
        &mut self,
        _: &Newline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.input.state();
        let buffer = self.input.buffer().read(cx);
        let edit = if state.selection().is_empty() && state.marked_range().is_none() {
            let Some(parsed_at) = self.parsed_at.filter(|&at| at == buffer.version()) else {
                return;
            };
            inkra_markdown::continue_list(buffer.text(), self.markdown(), state.cursor())
                .map(|edit| (Diff::new(parsed_at, edit.hunks), edit.caret))
        } else {
            None
        };
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| match edit {
            Some((diff, caret)) => {
                // The new parse snaps the caret in `refresh`; the old one would read old atoms.
                if state.apply_diff(buffer, &diff) {
                    state.move_to(buffer, caret, false, inkra_editor::no_snap);
                }
            }
            None => state.insert(buffer, now, "\n"),
        });
    }
}
