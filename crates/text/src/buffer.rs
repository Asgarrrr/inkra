use std::cell::Cell;
use std::ops::Range;

use crate::anchor::AnchorState;
use crate::offset::{ByteOffset, Utf16Offset};

mod anchors;
mod graphemes;
mod history;
mod lines;
mod utf16;

use history::Transaction;
pub use history::{GROUP_INTERVAL, Txn, Version};
use utf16::utf16_len;

pub struct Buffer {
    text: String,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    grouping_blocked: bool,
    next_edit_id: u64,
    /// Every anchor ever created, `None` once forgotten; slots are never reused.
    anchors: Vec<Option<AnchorState>>,
    /// The last conversion's answer; the next walks only from here.
    utf16: Cell<(ByteOffset, Utf16Offset)>,
}

impl Buffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            grouping_blocked: false,
            next_edit_id: 1,
            anchors: Vec::new(),
            utf16: Cell::new((ByteOffset::default(), Utf16Offset::default())),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// # Panics
    /// Panics if `range` is out of bounds or does not fall on char boundaries.
    #[must_use]
    pub fn slice(&self, range: Range<ByteOffset>) -> &str {
        &self.text[range.start.get()..range.end.get()]
    }

    #[must_use]
    pub fn len(&self) -> ByteOffset {
        ByteOffset::new(self.text.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The one text mutation, so the anchors and the UTF-16 checkpoint follow every edit.
    fn splice(&mut self, at: ByteOffset, removed: &str, inserted: &str) {
        let (byte, utf16) = self.utf16.get();
        let end = at + removed.len();
        if byte > at {
            self.utf16.set(if byte >= end {
                (
                    byte - removed.len() + inserted.len(),
                    utf16 - utf16_len(removed) + utf16_len(inserted),
                )
            } else {
                (at, utf16 - utf16_len(&removed[..byte - at]))
            });
        }
        self.text.replace_range(at.get()..end.get(), inserted);
        self.shift_anchors(at, removed.len(), inserted.len());
    }
}

#[cfg(test)]
mod tests;
