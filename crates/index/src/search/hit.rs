use std::ops::Range;

use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use crate::window::around;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub note: EntryId,
    pub range: Range<ByteOffset>,
    /// Part of the hit's line, for display only; it always holds the whole hit.
    pub context: String,
    /// The hit inside `context`.
    pub context_match: Range<usize>,
}

impl Hit {
    pub(crate) fn new(note: EntryId, text: &str, range: Range<ByteOffset>) -> Self {
        let (start, end) = (range.start.0, range.end.0);
        let window = around(text, start..end);
        Self {
            note,
            range,
            context: text[window.clone()].to_owned(),
            context_match: start - window.start..end - window.start,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Searched {
    pub hits: Vec<Hit>,
    /// At least `limit` hits exist; there may be more.
    pub limit_reached: bool,
}

impl Searched {
    /// Joins the ranges' results in range order; equals one `search` over their union.
    #[must_use]
    pub fn merge(chunks: Vec<Searched>, limit: usize) -> Searched {
        let mut hits: Vec<Hit> = chunks.into_iter().flat_map(|chunk| chunk.hits).collect();
        hits.truncate(limit);
        let limit_reached = hits.len() >= limit;
        Searched {
            hits,
            limit_reached,
        }
    }
}
