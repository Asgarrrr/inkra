use std::collections::HashMap;
use std::iter;

use foldhash::fast::RandomState;

use super::NoteWords;
use super::note_words::first_at_or_after;
use super::ordinal::{Alive, Ordinal};
use super::{narrow, varint, wide};

/// The notes as of the last merge, in sorted arrays; every snapshot shares it.
#[derive(Debug)]
pub(super) struct Segment {
    /// The distinct words, sorted and concatenated; word `w` is `bytes[bounds[w]..bounds[w + 1]]`.
    bytes: Box<str>,
    bounds: Box<[u32]>,
    /// Word `w`'s postings are `postings[starts[w]..starts[w + 1]]`: varint (gap, count) pairs.
    starts: Box<[u32]>,
    postings: Box<[u8]>,
    /// How many notes have postings here, live or not.
    notes: usize,
    /// Per ordinal, the live note's words with repeats; 0 for a dead or absent one.
    lengths: Box<[u32]>,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            bytes: Box::default(),
            bounds: [0].into(),
            starts: [0].into(),
            postings: Box::default(),
            notes: 0,
            lengths: Box::default(),
        }
    }
}

impl Segment {
    /// `self`'s `alive` postings plus `fresh`'s, sorted by ordinal and none alive, in one pass.
    pub(super) fn merge(&self, alive: &Alive, fresh: &[(Ordinal, &NoteWords)]) -> Self {
        let added = invert(fresh);
        let added_bytes: usize = added.iter().map(|(_, run)| run.bytes.len()).sum();
        let mut out = Builder::with_capacity(self.bytes.len(), self.postings.len() + added_bytes);
        let mut old = self.words().peekable();
        let mut new = added
            .iter()
            .map(|(word, run)| (*word, run.postings()))
            .peekable();
        loop {
            let word = match (old.peek(), new.peek()) {
                (Some(&(kept, _)), Some(&(added, _))) => kept.min(added),
                (Some(&(kept, _)), None) => kept,
                (None, Some(&(added, _))) => added,
                (None, None) => break,
            };
            let kept = old
                .next_if(|&(kept, _)| kept == word)
                .map(|(_, postings)| postings)
                .unwrap_or_default()
                .filter(|&(ordinal, _)| alive.contains(ordinal));
            let added = new
                .next_if(|&(added, _)| added == word)
                .map(|(_, postings)| postings)
                .unwrap_or_default();
            out.push(word, merged(kept, added));
        }
        out.finish(alive.len() + fresh.len(), self.merged_lengths(alive, fresh))
    }

    /// The words of `ordinal`'s note, repeats counted, as of the merge; 0 when it has none here.
    pub(super) fn length(&self, ordinal: Ordinal) -> u32 {
        self.lengths.get(ordinal.index()).copied().unwrap_or(0)
    }

    fn merged_lengths(&self, alive: &Alive, fresh: &[(Ordinal, &NoteWords)]) -> Box<[u32]> {
        let last = alive
            .iter()
            .chain(fresh.iter().map(|&(ordinal, _)| ordinal))
            .max();
        let mut lengths = vec![0; last.map_or(0, |ordinal| ordinal.index() + 1)];
        for ordinal in alive.iter() {
            lengths[ordinal.index()] = self.length(ordinal);
        }
        for &(ordinal, words) in fresh {
            lengths[ordinal.index()] = words.length();
        }
        lengths.into()
    }

    /// The words starting with `term` and their postings: one binary search, then a run.
    pub(super) fn with_prefix<'a>(
        &'a self,
        term: &'a str,
    ) -> impl Iterator<Item = (&'a str, Postings<'a>)> + 'a {
        let first = first_at_or_after(self.len(), |w| self.word(w), term);
        (first..self.len())
            .map(|w| self.entry(w))
            .take_while(move |(word, _)| word.starts_with(term))
    }

    /// Every word and its postings, in order.
    pub(super) fn words(&self) -> impl Iterator<Item = (&str, Postings<'_>)> {
        (0..self.len()).map(|w| self.entry(w))
    }

    pub(super) fn notes(&self) -> usize {
        self.notes
    }

    /// Its words, bounds and postings, for `WordsSnapshot::heap_bytes`.
    pub(super) fn heap_bytes(&self) -> usize {
        self.bytes.len()
            + size_of_val(&*self.bounds)
            + size_of_val(&*self.starts)
            + self.postings.len()
            + size_of_val(&*self.lengths)
    }

    fn len(&self) -> usize {
        self.bounds.len() - 1
    }

    fn entry(&self, w: usize) -> (&str, Postings<'_>) {
        let bytes = &self.postings[wide(self.starts[w])..wide(self.starts[w + 1])];
        (self.word(w), Postings { bytes, last: 0 })
    }

    fn word(&self, w: usize) -> &str {
        &self.bytes[wide(self.bounds[w])..wide(self.bounds[w + 1])]
    }
}

/// One word's `(ordinal, count)` pairs, decoded as read.
#[derive(Debug, Clone, Default)]
pub(super) struct Postings<'a> {
    bytes: &'a [u8],
    last: u32,
}

impl Iterator for Postings<'_> {
    type Item = (Ordinal, u32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        self.last += varint::read(&mut self.bytes);
        let count = varint::read(&mut self.bytes);
        Some((Ordinal::new(self.last), count))
    }
}

/// A `Segment` under construction, one word at a time in order.
struct Builder {
    bytes: String,
    bounds: Vec<u32>,
    starts: Vec<u32>,
    postings: Vec<u8>,
}

impl Builder {
    fn with_capacity(bytes: usize, postings: usize) -> Self {
        Self {
            bytes: String::with_capacity(bytes),
            bounds: vec![0],
            starts: vec![0],
            postings: Vec::with_capacity(postings),
        }
    }

    /// Appends `word` with `postings`, ascending by ordinal, or nothing when there are none.
    fn push(&mut self, word: &str, postings: impl Iterator<Item = (Ordinal, u32)>) {
        let before = self.postings.len();
        let mut last = 0;
        for (ordinal, count) in postings {
            encode(&mut self.postings, &mut last, ordinal, count);
        }
        if self.postings.len() > before {
            self.bytes.push_str(word);
            self.bounds.push(narrow(self.bytes.len()));
            self.starts.push(narrow(self.postings.len()));
        }
    }

    fn finish(self, notes: usize, lengths: Box<[u32]>) -> Segment {
        Segment {
            bytes: self.bytes.into(),
            bounds: self.bounds.into(),
            starts: self.starts.into(),
            postings: self.postings.into(),
            notes,
            lengths,
        }
    }
}

/// `fresh`'s distinct words, sorted, with their postings: hashing beats a k-way merge of notes.
fn invert<'a>(fresh: &[(Ordinal, &'a NoteWords)]) -> Vec<(&'a str, Run)> {
    debug_assert!(fresh.is_sorted_by_key(|&(ordinal, _)| ordinal));
    // Seeded, as `NoteWords::parse`: a note's words may come from anyone.
    let mut runs: HashMap<&str, Run, RandomState> = HashMap::default();
    for &(ordinal, words) in fresh {
        for (word, count) in words.iter() {
            let run = runs.entry(word).or_default();
            encode(&mut run.bytes, &mut run.last, ordinal, count);
        }
    }
    let mut sorted: Vec<(&str, Run)> = runs.into_iter().collect();
    sorted.sort_unstable_by_key(|&(word, _)| word);
    sorted
}

/// One fresh word's postings, encoded as the segment encodes them.
#[derive(Default)]
struct Run {
    bytes: Vec<u8>,
    last: u32,
}

impl Run {
    fn postings(&self) -> Postings<'_> {
        Postings {
            bytes: &self.bytes,
            last: 0,
        }
    }
}

/// Appends one posting after `last`, the run's previous ordinal (0 before the first).
fn encode(out: &mut Vec<u8>, last: &mut u32, ordinal: Ordinal, count: u32) {
    varint::push(out, ordinal.get() - *last);
    varint::push(out, count);
    *last = ordinal.get();
}

/// Two ascending runs as one; no ordinal is in both, since `fresh` holds no `alive` ordinal.
fn merged<T: Ord>(
    a: impl IntoIterator<Item = T>,
    b: impl IntoIterator<Item = T>,
) -> impl Iterator<Item = T> {
    let (mut a, mut b) = (a.into_iter().peekable(), b.into_iter().peekable());
    iter::from_fn(move || match (a.peek(), b.peek()) {
        (Some(x), Some(y)) if y < x => b.next(),
        (Some(_), _) => a.next(),
        (None, _) => b.next(),
    })
}

#[cfg(test)]
mod tests;
