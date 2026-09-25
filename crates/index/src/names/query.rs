use std::collections::HashMap;

use inkra_vault::EntryId;
use unicode_normalization::UnicodeNormalization;

use super::bag::CharBag;
use super::fold::QueryChar;

/// Matching costs terms × term chars × path chars, and dedup does not bound distinct terms.
const QUERY_MAX_CHARS: usize = 64;

/// A quick-open query, prepared once per keystroke and shared by every range.
#[derive(Debug, Clone)]
pub struct NameQuery {
    pub(super) terms: Vec<Vec<QueryChar>>,
    /// The terms as typed, repeats kept, joined by one space; a stem equal to it gains the exact-stem bonus.
    pub(super) joined: Vec<QueryChar>,
    pub(super) bag: CharBag,
    pub(super) showing: Option<EntryId>,
    recent: HashMap<EntryId, usize>,
}

impl NameQuery {
    /// `recent` is newest first; `showing` is the note on screen, ranked down.
    #[must_use]
    pub fn new(text: &str, showing: Option<EntryId>, recent: &[EntryId]) -> Self {
        let text: String = text.nfc().take(QUERY_MAX_CHARS).collect();
        let typed: Vec<Vec<QueryChar>> = text
            .split_whitespace()
            .map(|term| term.chars().map(QueryChar::new).collect())
            .collect();
        let joined = typed.join(&QueryChar::new(' '));
        let mut terms: Vec<Vec<QueryChar>> = Vec::with_capacity(typed.len());
        for term in typed {
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
        let mut bag = CharBag::default();
        for c in terms.iter().flatten() {
            bag.insert(c.c);
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
            joined,
            bag,
            showing,
            recent,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub(super) fn recent(&self, id: EntryId) -> Option<usize> {
        self.recent.get(&id).copied()
    }
}
