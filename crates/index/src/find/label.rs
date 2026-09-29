//! Word-start matching on one short label, as `⌘P` matches a name (command palette spec, D3).

use std::ops::Range;

use super::{light, word_prefix};
use crate::words::{NoteQuery, fold, word_spans};

/// Each lit byte range of `text` when every term of `query` starts one of its words, as `⌘P`
/// matches a name; `None` when a term starts none.
#[must_use]
pub fn word_starts(query: &NoteQuery, text: &str) -> Option<Vec<Range<usize>>> {
    let terms = query.terms();
    let mut started = vec![false; terms.len()];
    let mut lit = Vec::new();
    for (at, word) in word_spans(text) {
        let folded = fold(word);
        for (term, started) in terms.iter().zip(&mut started) {
            if folded.starts_with(&**term) {
                *started = true;
                light(&mut lit, word_prefix(text, at..at + word.len(), term));
            }
        }
    }
    (!started.contains(&false)).then_some(lit)
}
