//! Inputs the module tests share.

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::{Language, TokenClass, highlight};

pub(crate) fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

/// Two blocks with different shapes: `let` is a keyword at 8..11, `mut` one at 23..26.
pub(crate) const TWO_BLOCKS: &str = "```rust\nlet\n```\n\n```rs\nmut x\n```\n";

/// The tokens of `text` in `language`, each line at its offset in `text`.
pub(crate) fn tokens(language: Language, text: &str) -> Vec<(usize, usize, TokenClass)> {
    let mut start = 0;
    let lines = text.split('\n').map(|line| {
        let at = start;
        start += line.len() + 1;
        (ByteOffset::new(at), line)
    });
    let mut out = Vec::new();
    highlight(language, lines, &mut out);
    out.into_iter()
        .map(|(range, class)| (range.start.get(), range.end.get(), class))
        .collect()
}

/// Asserts the tokens of `text` in `language`: each expected piece, found in order in `text`
/// after the one before it, with its class.
#[track_caller]
pub(crate) fn expect(language: Language, text: &str, expected: &[(&str, TokenClass)]) {
    let mut from = 0;
    let wanted: Vec<_> = expected
        .iter()
        .map(|&(piece, class)| {
            let Some(at) = text[from..].find(piece).map(|at| from + at) else {
                panic!("{piece:?} not found after {from} in {text:?}");
            };
            from = at + piece.len();
            (at, from, class)
        })
        .collect();
    assert_eq!(tokens(language, text), wanted, "{text:?}");
}
