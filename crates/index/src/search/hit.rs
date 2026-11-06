use std::ops::Range;

use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use crate::window::around;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub range: Range<ByteOffset>,
    /// 1-based logical line of the hit's start.
    pub line: usize,
    /// Part of the hit's line, for display only; it always holds the whole hit.
    pub context: String,
    /// The hit inside `context`.
    pub context_match: Range<usize>,
}

impl Hit {
    pub(crate) fn new(text: &str, range: Range<ByteOffset>, line: usize) -> Self {
        let (start, end) = (range.start.get(), range.end.get());
        let window = around(text, start..end);
        Self {
            range,
            line,
            context: text[window.clone()].to_owned(),
            context_match: start - window.start..end - window.start,
        }
    }
}

/// One note's hits, never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteHits {
    pub note: EntryId,
    pub hits: Vec<Hit>,
    /// The note has more hits than `hits` lists.
    pub capped: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Searched {
    /// In vault order.
    pub notes: Vec<NoteHits>,
    /// At least `limit` hits exist; there may be more.
    pub limit_reached: bool,
}
