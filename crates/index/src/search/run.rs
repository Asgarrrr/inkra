use std::cell::LazyCell;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_text::ByteOffset;

use super::hit::{Hit, Searched};
use crate::find::{Terms, each_word, light, word_prefix};
use crate::index::IndexSnapshot;
use crate::words::{Lookup, NoteQuery};

/// A note gives at most this many hits; slice 6 shows "100+" on a capped note.
pub const HITS_PER_NOTE: usize = 100;

/// The first `limit` word starts of the query's terms in `range`, in vault order, from the
/// notes holding every term. Pass the `lookup` of this `snapshot` and `query`. Empty when
/// `cancel` is set, and without a hit when the query has no term.
#[must_use]
pub fn search(
    snapshot: &IndexSnapshot,
    lookup: &Lookup,
    range: Range<usize>,
    query: &NoteQuery,
    cancel: &AtomicBool,
    limit: usize,
) -> Searched {
    if cancel.load(Ordering::Relaxed) {
        return Searched::default();
    }
    if query.is_empty() {
        // No note can hit; `limit_reached` keeps its rule, so `merge` equals one range.
        return Searched {
            limit_reached: limit == 0,
            ..Searched::default()
        };
    }
    // Built at the first candidate: a query no note holds pays only the lookup.
    let terms = LazyCell::new(|| Terms::new(query));
    let mut hits = Vec::new();
    for (note, text) in snapshot.corpus().notes(range) {
        if hits.len() == limit {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            return Searched::default();
        }
        let holds = lookup.ordinal(note).is_some_and(|ordinal| {
            (0..lookup.term_count()).all(|term| lookup.hit(term, ordinal).is_some())
        });
        if !holds {
            continue;
        }
        let room = HITS_PER_NOTE.min(limit - hits.len());
        let Some(lit) = locate(text, &terms, room, cancel) else {
            return Searched::default();
        };
        hits.extend(
            lit.into_iter()
                .map(|range| Hit::new(note, text, ByteOffset(range.start)..ByteOffset(range.end))),
        );
    }
    let limit_reached = hits.len() >= limit;
    Searched {
        hits,
        limit_reached,
    }
}

/// The first `room` hits in `text`; `None` once `cancel` is set.
fn locate(
    text: &str,
    terms: &Terms<'_>,
    room: usize,
    cancel: &AtomicBool,
) -> Option<Vec<Range<usize>>> {
    let mut lit = Vec::new();
    each_word(text, cancel, |word| {
        // Terms starting one word are prefixes of each other: the longest lights the most.
        if let Some(term) = terms
            .matching(&text[word.clone()])
            .max_by_key(|term| term.len())
        {
            light(&mut lit, word_prefix(text, word, term));
        }
        // One hit past `room`: until then, a later word may still merge into the last one.
        if lit.len() > room { 0 } else { usize::MAX }
    })?;
    lit.truncate(room);
    Some(lit)
}
