use std::cell::LazyCell;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use super::hit::{Hit, NoteHits, Searched};
use crate::find::{Terms, each_word, light, word_prefix};
use crate::index::IndexSnapshot;
use crate::words::{Lookup, NoteQuery};

/// A note gives at most this many hits; a capped `NoteHits` says it was cut short.
pub const HITS_PER_NOTE: usize = 100;

/// The first `limit` word starts of the query's terms in `range`, grouped by note in vault
/// order. Pass the `lookup` of this `snapshot` and `query`. Empty once `cancel` is set or with no term.
#[must_use]
pub(crate) fn search_in(
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
    let mut notes = Vec::new();
    let mut count = 0;
    for (note, text) in snapshot.corpus().notes(range) {
        if count == limit {
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
        let room = HITS_PER_NOTE.min(limit - count);
        let Some(group) = note_hits(note, text, &terms, room, cancel) else {
            return Searched::default();
        };
        count += group.hits.len();
        notes.push(group);
    }
    Searched {
        notes,
        limit_reached: count >= limit,
    }
}

/// The first `room` hits of `note`'s `text`, capped when it has more; `None` once `cancel` is set.
pub(super) fn note_hits(
    note: EntryId,
    text: &str,
    terms: &Terms<'_>,
    room: usize,
    cancel: &AtomicBool,
) -> Option<NoteHits> {
    let mut lit = locate(text, terms, room, cancel)?;
    let capped = lit.len() > room;
    lit.truncate(room);
    let hits = lines(text, lit)
        .map(|(range, line)| {
            Hit::new(
                text,
                ByteOffset::new(range.start)..ByteOffset::new(range.end),
                line,
            )
        })
        .collect();
    Some(NoteHits { note, hits, capped })
}

/// The first `room` hits in `text`, plus one if more exist; `None` once `cancel` is set.
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
    Some(lit)
}

/// Each range of `lit`, ascending, with the 1-based line of its start.
fn lines(text: &str, lit: Vec<Range<usize>>) -> impl Iterator<Item = (Range<usize>, usize)> {
    let (mut from, mut line) = (0, 1);
    lit.into_iter().map(move |range| {
        line += text[from..range.start].matches('\n').count();
        from = range.start;
        (range, line)
    })
}
