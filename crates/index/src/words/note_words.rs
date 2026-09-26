use std::borrow::Cow;
use std::collections::HashMap;

use foldhash::fast::RandomState;

use super::{WORD_MAX_CHARS, cap, fold, word_spans};

/// A note's distinct folded words, each capped, sorted, with how often each occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteWords {
    /// The words, concatenated in order; word `n` is `bytes[bounds[n]..bounds[n + 1]]`.
    bytes: Box<str>,
    bounds: Box<[usize]>,
    counts: Box<[u32]>,
}

impl Default for NoteWords {
    fn default() -> Self {
        Self {
            bytes: Box::default(),
            bounds: Box::new([0]),
            counts: Box::default(),
        }
    }
}

impl NoteWords {
    /// Counts each spelling, then folds each distinct one once: a note repeats its words.
    pub(crate) fn parse(text: &str) -> Self {
        // Seeded: a note may come from anyone, and a seed-free hash lets its words collide.
        let mut spelled: HashMap<&str, u32, RandomState> = HashMap::default();
        for (_, word) in word_spans(text) {
            let count = spelled.entry(word).or_default();
            *count = count.saturating_add(1);
        }
        let mut folded: Vec<(Cow<'_, str>, u32)> = spelled
            .into_iter()
            .map(|(word, count)| (capped(fold(word)), count))
            .filter(|(word, _)| !word.is_empty())
            .collect();
        folded.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        folded.dedup_by(|(word, count), (kept, sum)| {
            let equal = word == kept;
            if equal {
                *sum = sum.saturating_add(*count);
            }
            equal
        });
        let mut bytes = String::with_capacity(folded.iter().map(|(word, _)| word.len()).sum());
        let mut bounds = Vec::with_capacity(folded.len() + 1);
        bounds.push(0);
        let mut counts = Vec::with_capacity(folded.len());
        for (word, count) in folded {
            bytes.push_str(&word);
            bounds.push(bytes.len());
            counts.push(count);
        }
        Self {
            bytes: bytes.into(),
            bounds: bounds.into(),
            counts: counts.into(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Every word and its count, in order.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = (&str, u32)> + '_ {
        (0..self.counts.len()).map(|n| (self.word(n), self.counts[n]))
    }

    /// The words starting with `term`, in order: one binary search, then a run.
    pub(crate) fn with_prefix<'a>(
        &'a self,
        term: &'a str,
    ) -> impl Iterator<Item = (&'a str, u32)> + 'a {
        let first = first_at_or_after(self.counts.len(), |n| self.word(n), term);
        (first..self.counts.len())
            .map(|n| (self.word(n), self.counts[n]))
            .take_while(move |(word, _)| word.starts_with(term))
    }

    /// Its bytes, `bounds` and `counts`, for `WordsSnapshot::heap_bytes`.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.bytes.len() + size_of_val(&*self.bounds) + size_of_val(&*self.counts)
    }

    fn word(&self, n: usize) -> &str {
        &self.bytes[self.bounds[n]..self.bounds[n + 1]]
    }
}

/// The first of `len` sorted words not below `term`, by binary search; `word(n)` reads word `n`.
pub(super) fn first_at_or_after<'a>(
    len: usize,
    word: impl Fn(usize) -> &'a str,
    term: &str,
) -> usize {
    let (mut low, mut high) = (0, len);
    while low < high {
        let mid = low + (high - low) / 2;
        if word(mid) < term {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

/// `word` cut to `WORD_MAX_CHARS`, without a second allocation when the fold made one.
fn capped(word: Cow<'_, str>) -> Cow<'_, str> {
    match word {
        Cow::Borrowed(word) => Cow::Borrowed(cap(word, WORD_MAX_CHARS)),
        Cow::Owned(mut word) => {
            word.truncate(cap(&word, WORD_MAX_CHARS).len());
            Cow::Owned(word)
        }
    }
}
