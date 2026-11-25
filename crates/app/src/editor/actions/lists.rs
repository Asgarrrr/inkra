//! The list commands' handlers: each asks `inkra-markdown` for a `ListEdit` on the cached parse
//! and applies it as one step (list editing spec, D9).

use std::ops::Range;
use std::time::Instant;

use gpui::{Context, Window};
use inkra_markdown::{ListEdit, Markdown};
use inkra_text::{Bias, ByteOffset, Diff};

use super::{IndentItem, Newline, OutdentItem};
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
                    let caret = diff.map(caret, Bias::Right);
                    state.move_to(buffer, caret, false, inkra_editor::no_snap);
                }
            }
            None => state.insert(buffer, now, "\n"),
        });
    }

    /// Off a list, a caret takes the plain `\t`, so code keeps its tab; a selection takes nothing
    /// (D4).
    pub(in crate::editor) fn indent_item(
        &mut self,
        _: &IndentItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_items(inkra_markdown::indent_items, true, window, cx);
    }

    /// Off a list, nothing: `⇧⇥` never writes a byte (D4).
    pub(in crate::editor) fn outdent_item(
        &mut self,
        _: &OutdentItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_items(inkra_markdown::outdent_items, false, window, cx);
    }

    /// `apply_diff` maps the selection, its head included (D9); an edit with no hunks takes no
    /// lease, as `toggle_task`'s does.
    fn move_items(
        &mut self,
        command: fn(&str, &Markdown, Range<ByteOffset>) -> Option<ListEdit>,
        tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.input.state();
        let buffer = self.input.buffer().read(cx);
        let plain = tab && state.selection().is_empty();
        let edit = if state.marked_range().is_none() {
            let Some(parsed_at) = self.parsed_at.filter(|&at| at == buffer.version()) else {
                return;
            };
            command(buffer.text(), self.markdown(), state.selection().range())
                .map(|edit| Diff::new(parsed_at, edit.hunks))
        } else {
            None
        };
        match edit {
            Some(diff) if diff.hunks().is_empty() => {}
            Some(diff) => {
                self.edit_buffer(window, cx, |state, buffer| state.apply_diff(buffer, &diff));
            }
            None if plain => {
                let now = Instant::now();
                self.edit_buffer(window, cx, |state, buffer| state.insert(buffer, now, "\t"));
            }
            None => {}
        }
    }
}
