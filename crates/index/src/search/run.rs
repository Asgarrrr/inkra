use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use super::hit::{Hit, Searched};
use super::query::TextQuery;
use crate::corpus::CorpusSnapshot;

/// A note gives at most this many hits; slice 6 shows "100+" on a capped note.
pub const HITS_PER_NOTE: usize = 100;

/// The first `limit` hits in `range`, in vault order. Empty when `cancel` is set.
#[must_use]
pub fn search(
    corpus: &CorpusSnapshot,
    range: Range<usize>,
    query: &TextQuery,
    cancel: &AtomicBool,
    limit: usize,
) -> Searched {
    if cancel.load(Ordering::Relaxed) {
        return Searched::default();
    }
    let mut hits = Vec::new();
    for (note, text) in corpus.notes(range) {
        if cancel.load(Ordering::Relaxed) {
            return Searched::default();
        }
        let room = HITS_PER_NOTE.min(limit - hits.len());
        hits.extend(
            query
                .find_in(text, room)
                .into_iter()
                .map(|range| Hit::new(note, text, range)),
        );
        if hits.len() == limit {
            break;
        }
    }
    let limit_reached = hits.len() >= limit;
    Searched {
        hits,
        limit_reached,
    }
}
