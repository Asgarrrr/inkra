use std::ops::Range;
use std::time::Instant;

use similar::{Algorithm, DiffTag, capture_diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::anchor::{Bias, transform};
use crate::buffer::Buffer;
use crate::offset::ByteOffset;

// Not Zed's histogram: `similar`'s took 65x Myers' time on a 1 MB note (watcher spec, F8).
const ALGORITHM: Algorithm = Algorithm::Myers;
// Zed's caps (`language/src/text_diff.rs:6-7`): past them a word diff scatters into noise.
const MAX_WORD_DIFF_LEN: usize = 512;
const MAX_WORD_DIFF_LINE_COUNT: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Whitespace,
    Punctuation,
}

/// One replacement: `range` in the old text becomes `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub range: Range<ByteOffset>,
    pub text: String,
}

/// The hunks that turn a buffer's text at one version into a new text, as Zed's `Diff`.
/// Only [`diff`] builds one, so its hunks are in order, disjoint, and each edits something.
#[derive(Debug)]
pub struct Diff {
    base_version: u64,
    hunks: Vec<Hunk>,
}

impl Diff {
    #[must_use]
    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    /// Where `offset` in the base text lands once the diff applies: where an anchor with `bias`
    /// would resolve, since `apply_diff` replays the hunks in the same order.
    #[must_use]
    pub fn map(&self, offset: ByteOffset, bias: Bias) -> ByteOffset {
        self.hunks.iter().rev().fold(offset, |offset, hunk| {
            let removed = hunk.range.end.0 - hunk.range.start.0;
            transform(offset, bias, hunk.range.start, removed, hunk.text.len())
        })
    }
}

/// The diff from `old`, the text of a buffer at `base_version`, to `new`.
/// A line diff, refined to words inside small hunks, as Zed's `text_diff` does.
#[must_use]
pub fn diff(base_version: u64, old: &str, new: &str) -> Diff {
    let mut hunks = Vec::new();
    diff_tokens(
        &old.split_inclusive('\n').collect::<Vec<_>>(),
        &new.split_inclusive('\n').collect::<Vec<_>>(),
        |old_range, new_range, old_lines, new_lines| {
            let (old_hunk, new_hunk) = (&old[old_range.clone()], &new[new_range.clone()]);
            if word_diffable(old_hunk, old_lines) && word_diffable(new_hunk, new_lines) {
                diff_tokens(
                    &tokenize(old_hunk),
                    &tokenize(new_hunk),
                    |old_word, new_word, _, _| {
                        hunks.push(Hunk {
                            range: byte_range(
                                old_range.start + old_word.start..old_range.start + old_word.end,
                            ),
                            text: new_hunk[new_word].to_string(),
                        });
                    },
                );
            } else {
                hunks.push(Hunk {
                    range: byte_range(old_range),
                    text: new_hunk.to_string(),
                });
            }
        },
    );
    Diff {
        base_version,
        hunks,
    }
}

fn word_diffable(text: &str, lines: usize) -> bool {
    text.len() <= MAX_WORD_DIFF_LEN && lines <= MAX_WORD_DIFF_LINE_COUNT
}

fn byte_range(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset(range.start)..ByteOffset(range.end)
}

/// Calls `on_hunk(old_range, new_range, old_token_count, new_token_count)` for each changed
/// run: its byte ranges in `old` and `new`, then how many tokens it spans in each.
fn diff_tokens(
    old: &[&str],
    new: &[&str],
    mut on_hunk: impl FnMut(Range<usize>, Range<usize>, usize, usize),
) {
    let (mut old_at, mut new_at) = (0, 0);
    // No deadline: it reads a clock, which `text` may not.
    for op in capture_diff_slices(ALGORITHM, old, new) {
        let (tag, old_tokens, new_tokens) = op.as_tag_tuple();
        let old_len: usize = old[old_tokens.clone()]
            .iter()
            .map(|token| token.len())
            .sum();
        let new_len: usize = new[new_tokens.clone()]
            .iter()
            .map(|token| token.len())
            .sum();
        if tag != DiffTag::Equal {
            on_hunk(
                old_at..old_at + old_len,
                new_at..new_at + new_len,
                old_tokens.len(),
                new_tokens.len(),
            );
        }
        old_at += old_len;
        new_at += new_len;
    }
}

/// A grapheme takes its first char's kind, so a combining mark or a ZWJ never leaves its base.
fn grapheme_kind(grapheme: &str) -> TokenKind {
    match grapheme.chars().next() {
        Some(c) if c.is_alphanumeric() || c == '_' => TokenKind::Word,
        Some(c) if c.is_whitespace() => TokenKind::Whitespace,
        _ => TokenKind::Punctuation,
    }
}

/// Runs of word graphemes, of whitespace, or of one repeated punctuation grapheme: Zed's
/// `tokenize` by grapheme instead of by char, so no hunk splits a grapheme.
fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut graphemes = text.grapheme_indices(true);
    let Some((_, mut run_head)) = graphemes.next() else {
        return tokens;
    };
    let (mut start, mut current) = (0, grapheme_kind(run_head));
    for (index, grapheme) in graphemes {
        let next = grapheme_kind(grapheme);
        if next != current || (next == TokenKind::Punctuation && grapheme != run_head) {
            tokens.push(&text[start..index]);
            (start, run_head, current) = (index, grapheme, next);
        }
    }
    tokens.push(&text[start..]);
    tokens
}

impl Buffer {
    /// Applies `diff` as one undo step that merges with no typing on either side.
    /// Returns false and edits nothing if this buffer's version moved since `diff`'s base:
    /// its hunks would land on the wrong text.
    #[must_use]
    pub fn apply_diff(&mut self, now: Instant, diff: &Diff) -> bool {
        if self.version() != diff.base_version {
            return false;
        }
        if diff.hunks.is_empty() {
            return true;
        }
        self.finalize_transaction();
        self.transact_at(now, |buffer| {
            // Last first, so each edit leaves the ranges before it valid.
            for hunk in diff.hunks.iter().rev() {
                buffer.replace(hunk.range.clone(), &hunk.text);
            }
        });
        self.finalize_transaction();
        true
    }
}

#[cfg(test)]
mod tests;
