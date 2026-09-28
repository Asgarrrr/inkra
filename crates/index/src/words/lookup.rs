use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::EntryId;

use super::ordinal::Ordinal;
use super::query::NoteQuery;
use super::score::{bm25, idf};
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
pub struct Lookup {
    /// The snapshot looked up, for the ordinals and the notes' lengths.
    words: WordsSnapshot,
    /// Per term, a hit per ordinal; a count of 0 is a note without the term.
    terms: Vec<Vec<TermHit>>,
    /// Per term, its rarity among the notes with words.
    idf: Vec<f64>,
    /// The mean words per note with words.
    avg: f64,
}

impl Lookup {
    /// How the note holds the query's `term`-th term; `None` when it holds no word starting with it.
    #[must_use]
    pub fn get(&self, term: usize, id: EntryId) -> Option<TermHit> {
        self.hit(term, self.ordinal(id)?)
    }

    /// The note's ordinal, resolved once for all its terms; `None` when it has no words.
    pub(crate) fn ordinal(&self, id: EntryId) -> Option<Ordinal> {
        self.words.ordinals.get(id).copied()
    }

    pub(crate) fn hit(&self, term: usize, ordinal: Ordinal) -> Option<TermHit> {
        let hit = self.terms.get(term)?.get(ordinal.index())?;
        (hit.count > 0).then_some(*hit)
    }

    /// The `term`-th term's BM25 weight in the note holding it as `hit`.
    pub(crate) fn score(&self, term: usize, ordinal: Ordinal, hit: TermHit) -> f64 {
        bm25(
            self.idf[term],
            hit.count,
            self.words.length(ordinal),
            self.avg,
        )
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
        let notes = self.live_notes();
        let mut terms = Vec::with_capacity(query.terms().len());
        let mut idfs = Vec::with_capacity(query.terms().len());
        for term in query.terms() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let (mut hits, mut df) = (Vec::new(), 0);
            for (word, postings) in self.segment.with_prefix(term) {
                let whole = word == &**term;
                for (ordinal, count) in postings {
                    if self.alive.contains(ordinal) && add(&mut hits, ordinal, count, whole) {
                        df += 1;
                    }
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            for (&ordinal, words) in self.fresh.iter() {
                for (word, count) in words.with_prefix(term) {
                    if add(&mut hits, ordinal, count, word == &**term) {
                        df += 1;
                    }
                }
            }
            terms.push(hits);
            idfs.push(idf(df, notes));
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a total past 2^53 words loses only low bits of the mean"
        )]
        let total = self.total as f64;
        Some(Lookup {
            words: self.clone(),
            terms,
            idf: idfs,
            avg: total / f64::from(notes),
        })
    }

    /// BM25's `N`: the live notes with words.
    fn live_notes(&self) -> u32 {
        let Ok(notes) = u32::try_from(self.alive.len() + self.fresh.len()) else {
            panic!("ordinals are u32, so the notes with words fit one");
        };
        notes
    }
}

/// Adds a word's `count` to the note's hit; true when the note had none, to count it once in `df`.
fn add(hits: &mut Vec<TermHit>, ordinal: Ordinal, count: u32, whole: bool) -> bool {
    let at = ordinal.index();
    if at >= hits.len() {
        hits.resize(at + 1, ABSENT);
    }
    let hit = &mut hits[at];
    let first = hit.count == 0;
    hit.count = hit.count.saturating_add(count);
    hit.whole |= whole;
    first
}
