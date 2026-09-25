use std::ops::Range;

use inkra_text::ByteOffset;
use inkra_vault::EntryId;

/// Bytes of the hit's line kept before and after it.
const BEFORE: usize = 60;
const AFTER: usize = 100;

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
        // Only the window is scanned for the line's ends: a line can be megabytes long.
        let from = text.ceil_char_boundary(start.saturating_sub(BEFORE));
        let to = text.floor_char_boundary((end + AFTER).min(text.len()));
        let from = text[from..start]
            .rfind('\n')
            .map_or(from, |at| from + at + 1);
        let to = text[end..to].find('\n').map_or(to, |at| end + at);
        Self {
            note,
            range,
            context: text[from..to].to_owned(),
            context_match: start - from..end - from,
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
