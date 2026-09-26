use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::EntryId;

use super::ordinal::Ordinal;
use super::query::NoteQuery;
use super::store::WordsSnapshot;

/// How a note holds one term: how many of its words start with it, and whether one equals it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermHit {
    pub count: u32,
    pub whole: bool,
}

const ABSENT: TermHit = TermHit {
    count: 0,
    whole: false,
};

/// A query's terms resolved in the word index, once per keystroke; every range reads it.
#[derive(Debug)]
pub struct Lookup {
    ordinals: Arc<HashMap<EntryId, Ordinal>>,
    /// Per term, a hit per ordinal; a count of 0 is a note without the term.
    terms: Vec<Vec<TermHit>>,
}

impl Lookup {
    /// How the note holds the query's `term`-th term; `None` when it holds no word starting with it.
    #[must_use]
    pub fn get(&self, term: usize, id: EntryId) -> Option<TermHit> {
        let ordinal = self.ordinals.get(&id)?;
        let hit = self.terms.get(term)?.get(ordinal.index())?;
        (hit.count > 0).then_some(*hit)
    }

    #[must_use]
    pub fn term_count(&self) -> usize {
        self.terms.len()
    }
}

impl WordsSnapshot {
    /// Each term's notes: one range of the segment, one per fresh note; `None` if cancelled.
    #[must_use]
    pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Option<Lookup> {
        let mut terms = Vec::with_capacity(query.terms().len());
        for term in query.terms() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let mut hits = Vec::new();
            for (word, postings) in self.segment.with_prefix(term) {
                let whole = word == &**term;
                for (ordinal, count) in postings {
                    if self.alive.contains(ordinal) {
                        add(&mut hits, ordinal, count, whole);
                    }
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            for (&ordinal, words) in self.fresh.iter() {
                for (word, count) in words.with_prefix(term) {
                    add(&mut hits, ordinal, count, word == &**term);
                }
            }
            terms.push(hits);
        }
        Some(Lookup {
            ordinals: Arc::clone(&self.ordinals),
            terms,
        })
    }
}

fn add(hits: &mut Vec<TermHit>, ordinal: Ordinal, count: u32, whole: bool) {
    let at = ordinal.index();
    if at >= hits.len() {
        hits.resize(at + 1, ABSENT);
    }
    let hit = &mut hits[at];
    hit.count = hit.count.saturating_add(count);
    hit.whole |= whole;
}
