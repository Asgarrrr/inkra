use std::collections::HashMap;

use foldhash::fast::RandomState;
use inkra_vault::EntryId;

use super::{TERM_MAX_CHARS, cap, fold, word_spans};

/// S1 bounds the typed query; folding can still lengthen a term, which `TERM_MAX_CHARS` bounds.
const QUERY_MAX_CHARS: usize = 64;

/// A quick-open query, folded once per keystroke and shared by every range.
#[derive(Debug, Clone)]
pub struct NoteQuery {
    terms: Vec<Box<str>>,
    /// Each typed word, repeats kept, as its index in `terms`.
    typed: Vec<usize>,
    showing: Option<EntryId>,
    recent: HashMap<EntryId, usize, RandomState>,
}

impl NoteQuery {
    /// `recent` is newest first; `showing`, the note on screen, ranks after its ties (S3 key 3);
    /// an empty query does not list it.
    #[must_use]
    pub fn new(text: &str, showing: Option<EntryId>, recent: &[EntryId]) -> Self {
        let cut = cap(text, QUERY_MAX_CHARS).len();
        let mut terms: Vec<Box<str>> = Vec::new();
        let mut typed = Vec::new();
        for (start, word) in word_spans(cap(text, QUERY_MAX_CHARS + 1)) {
            if start >= cut {
                break;
            }
            let split = start + word.len() > cut;
            // A cut word would match whole on its prefix: drop it, unless it is the only term.
            if split && !terms.is_empty() {
                break;
            }
            let folded = fold(&word[..word.len().min(cut - start)]);
            let term = cap(&folded, TERM_MAX_CHARS);
            if term.is_empty() {
                continue;
            }
            let at = if let Some(at) = terms.iter().position(|seen| **seen == *term) {
                at
            } else {
                terms.push(term.into());
                terms.len() - 1
            };
            typed.push(at);
        }
        // Reversed so a repeated id keeps its newest rank.
        let recent = recent
            .iter()
            .enumerate()
            .rev()
            .map(|(rank, id)| (*id, rank))
            .collect();
        Self {
            terms,
            typed,
            showing,
            recent,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub(crate) fn terms(&self) -> &[Box<str>] {
        &self.terms
    }

    /// The typed words, folded and capped as terms, in typed order with repeats.
    pub(crate) fn typed(&self) -> impl Iterator<Item = &str> {
        self.typed.iter().map(|&at| &*self.terms[at])
    }

    pub(crate) fn showing(&self) -> Option<EntryId> {
        self.showing
    }

    /// The note's place in the MRU, newest 0.
    pub(crate) fn recent(&self, id: EntryId) -> Option<usize> {
        self.recent.get(&id).copied()
    }
}
