use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use crate::chunks::split;

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
    fn new(id: EntryId, path: String) -> Self {
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
            path: path.into(),
            chars,
            bonus,
            stem,
            bag,
        }
    }
}

/// The quick-open candidates in vault order. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct NamesSnapshot(Arc<[Candidate]>);

impl NamesSnapshot {
    pub(crate) fn build(notes: impl IntoIterator<Item = (EntryId, String)>) -> Self {
        Self(
            notes
                .into_iter()
                .map(|(id, path)| Candidate::new(id, path))
                .collect(),
        )
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Ranges to match in parallel; `merge_names` over their results equals one range.
    #[must_use]
    pub fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        split(self.0.len(), n)
    }

    fn candidates(&self) -> &[Candidate] {
        &self.0
    }
}

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
