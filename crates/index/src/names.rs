use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use crate::chunks::split;
use crate::words::NameWords;

mod bag;
mod fold;
mod matching;
mod query;
mod score;

use bag::CharBag;
use fold::PathChar;

pub use matching::{NameMatch, match_names, merge_names};
pub use query::NameQuery;

/// One note as quick open matches it. Built once per `set_notes`, off the foreground.
struct Candidate {
    id: EntryId,
    /// NFC, `/`-separated, `.md` stripped.
    path: Arc<str>,
    chars: Box<[PathChar]>,
    /// Per char: the boundary and stem-start bonus, which depend on the path alone.
    bonus: Box<[u8]>,
    /// Char index where the stem starts.
    stem: usize,
    bag: CharBag,
}

impl Candidate {
    fn new(id: EntryId, path: Arc<str>) -> Self {
        let raw: Vec<char> = path.chars().collect();
        let stem = raw.iter().rposition(|&c| c == '/').map_or(0, |at| at + 1);
        let chars: Box<[PathChar]> = raw.iter().map(|&c| PathChar::new(c)).collect();
        let bonus = (0..raw.len())
            .map(|at| score::bonus(&raw, at, stem))
            .collect();
        let mut bag = CharBag::default();
        for c in &chars {
            bag.insert(c.exact);
            bag.insert(c.base);
        }
        Self {
            id,
            path,
            chars,
            bonus,
            stem,
            bag,
        }
    }
}

/// The quick-open candidates in vault order. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct NamesSnapshot {
    candidates: Arc<[Candidate]>,
    /// Read by `find`, never by `match_names`.
    words: Arc<NameWords>,
}

impl NamesSnapshot {
    pub(crate) fn build(notes: Vec<(EntryId, Arc<str>)>) -> Self {
        let words = NameWords::build(notes.iter().map(|(_, path)| &**path));
        Self {
            candidates: notes
                .into_iter()
                .map(|(id, path)| Candidate::new(id, path))
                .collect(),
            words: Arc::new(words),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Ranges to match in parallel; `merge_names` over their results equals one range.
    #[must_use]
    pub fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        split(self.candidates.len(), n)
    }

    /// The name words' bytes and arrays; the candidates are not counted.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.words.heap_bytes()
    }

    pub(crate) fn id(&self, order: usize) -> EntryId {
        self.candidates[order].id
    }

    pub(crate) fn path(&self, order: usize) -> &Arc<str> {
        &self.candidates[order].path
    }

    pub(crate) fn chars(&self, order: usize) -> usize {
        self.candidates[order].chars.len()
    }

    pub(crate) fn words(&self) -> &NameWords {
        &self.words
    }

    fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
}

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
