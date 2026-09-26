use super::{TERM_MAX_CHARS, cap, fold, word_spans};

/// S1 bounds the typed query; folding can still lengthen a term, which `TERM_MAX_CHARS` bounds.
const QUERY_MAX_CHARS: usize = 64;

/// A quick-open query, folded once per keystroke and shared by every range.
#[derive(Debug, Clone)]
pub struct NoteQuery {
    terms: Vec<Box<str>>,
}

impl NoteQuery {
    /// Splits `text` into words as a note's text is split, folds each, and drops repeats.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let cut = cap(text, QUERY_MAX_CHARS).len();
        let mut terms: Vec<Box<str>> = Vec::new();
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
            if !term.is_empty() && !terms.iter().any(|seen| **seen == *term) {
                terms.push(term.into());
            }
        }
        Self { terms }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub(crate) fn terms(&self) -> &[Box<str>] {
        &self.terms
    }
}
