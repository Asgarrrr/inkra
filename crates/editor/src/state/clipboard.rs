//! Copy, cut and paste: what `⌘C` takes, and where a paste lands.

use std::ops::Range;

use inkra_text::{Buffer, ByteOffset};

use super::EditorState;
use crate::selection::Goal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub text: String,
    pub whole_line: bool,
}

impl EditorState {
    /// What `⌘C` takes. Read-only: a copy must not repaint or mark the note dirty.
    #[must_use]
    pub fn copy(&self, buffer: &Buffer) -> Copied {
        self.target(buffer).1
    }

    /// `copy`, then removes what it took in a step of its own, so it can't swallow adjacent typing.
    pub fn cut(&mut self, buffer: &mut Buffer) -> Copied {
        let (range, copied) = self.target(buffer);
        self.edit_ungrouped(buffer, range, "");
        copied
    }

    pub fn paste(&mut self, buffer: &mut Buffer, copied: &Copied) {
        // An empty paste must not replace the selection with nothing.
        if copied.text.is_empty() {
            return;
        }

        if copied.whole_line && self.selection.is_empty() {
            let caret = self.cursor();
            let start = self.line(buffer).start;
            self.edit_ungrouped(buffer, start..start, &copied.text);
            // The insertion sits before the caret, so it keeps its column; `edit_ungrouped` alone would reset it to line start.
            self.selection
                .collapse_to(caret + copied.text.len(), Goal::None);
        } else {
            self.edit_ungrouped(buffer, self.selection.range(), &copied.text);
        }
    }

    /// On an unterminated last line, the text gains a `\n` the range doesn't cover, so a
    /// whole-line item is always a whole line, even pasted straight back.
    fn target(&self, buffer: &Buffer) -> (Range<ByteOffset>, Copied) {
        if !self.selection.is_empty() {
            let range = self.selection.range();
            let text = buffer.slice(range.start..range.end).to_string();
            return (
                range,
                Copied {
                    text,
                    whole_line: false,
                },
            );
        }

        let line = self.line(buffer);
        // `line_range` stops before the newline, so at most one terminator byte follows it.
        let terminated = line.end < buffer.len();
        let end = if terminated { line.end + 1 } else { line.end };
        let mut text = buffer.slice(line.start..end).to_string();
        if !terminated {
            text.push('\n');
        }

        (
            line.start..end,
            Copied {
                text,
                whole_line: true,
            },
        )
    }
}
