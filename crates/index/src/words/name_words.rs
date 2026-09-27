use std::ops::Range;

use super::{WORD_MAX_CHARS, cap, fold, narrow, wide, word_spans};

/// Every name's folded words in three arrays: one allocation per array, not one per word.
#[derive(Debug)]
pub(crate) struct NameWords {
    /// The folded words, concatenated in order; word `n` ends at `words[n].folded_end`.
    folded: Box<str>,
    words: Box<[Word]>,
    /// Name `n`'s words are `words[starts[n]..starts[n + 1]]`; one more start than names.
    starts: Box<[u32]>,
}

#[derive(Debug, Clone, Copy)]
struct Word {
    folded_end: u32,
    /// The word's bytes in its path.
    start: u32,
    end: u32,
    in_stem: bool,
}

/// One word of a note's name, folded as S1 folds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameWord<'a> {
    pub(crate) folded: &'a str,
    pub(crate) bytes: Range<usize>,
    /// After the last `/`.
    pub(crate) in_stem: bool,
}

impl NameWords {
    /// `paths` NFC, `.md` stripped, in vault order; a word folding to nothing is dropped.
    pub(crate) fn build<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut folded = String::new();
        let mut words = Vec::new();
        let mut starts = Vec::new();
        for path in paths {
            starts.push(narrow(words.len()));
            let stem = path.rfind('/').map_or(0, |at| at + 1);
            for (start, word) in word_spans(path) {
                let word_folded = fold(word);
                let capped = cap(&word_folded, WORD_MAX_CHARS);
                if capped.is_empty() {
                    continue;
                }
                folded.push_str(capped);
                words.push(Word {
                    folded_end: narrow(folded.len()),
                    start: narrow(start),
                    end: narrow(start + word.len()),
                    in_stem: start >= stem,
                });
            }
        }
        starts.push(narrow(words.len()));
        Self {
            folded: folded.into(),
            words: words.into(),
            starts: starts.into(),
        }
    }

    pub(crate) fn of(&self, order: usize) -> impl Iterator<Item = NameWord<'_>> {
        (wide(self.starts[order])..wide(self.starts[order + 1])).map(move |n| {
            let word = self.words[n];
            let folded_start = n
                .checked_sub(1)
                .map_or(0, |previous| wide(self.words[previous].folded_end));
            NameWord {
                folded: &self.folded[folded_start..wide(word.folded_end)],
                bytes: wide(word.start)..wide(word.end),
                in_stem: word.in_stem,
            }
        })
    }

    /// The folded words and both arrays.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.folded.len() + size_of_val(&*self.words) + size_of_val(&*self.starts)
    }
}

impl Default for NameWords {
    fn default() -> Self {
        Self::build([])
    }
}

#[cfg(test)]
mod tests;
