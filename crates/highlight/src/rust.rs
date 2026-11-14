//! Rust: comments, strings, char literals, lifetimes and keywords (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;
use crate::state::{LineState, Open};

mod literals;

/// `Self`, `true`, `false`, `union` and `macro_rules` are not keywords here.
const KEYWORDS: [&str; 35] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

/// A word byte is ASCII alphanumeric, `_` or non-ASCII, so a word ends on a char boundary.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii()
}

fn word_end(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| !is_word(byte))
        .map_or(bytes.len(), |len| at + len)
}

/// One line's tokens, pushed at the line's start offset.
struct Tokens<'a> {
    line: &'a str,
    start: ByteOffset,
    out: &'a mut Vec<(Range<ByteOffset>, TokenClass)>,
}

impl Tokens<'_> {
    fn push(&mut self, range: Range<usize>, class: TokenClass) {
        if !range.is_empty() {
            self.out
                .push((self.start + range.start..self.start + range.end, class));
        }
    }

    /// Scans `open` from `at` to its closer or the line end, as one token from `token`.
    fn literal(&mut self, open: Open, token: usize, at: usize) -> (usize, Open) {
        let bytes = self.line.as_bytes();
        let (end, open, class) = match open {
            Open::Nothing => return (at, open),
            Open::BlockComment { depth } => {
                let (end, open) = literals::block_comment(bytes, at, depth);
                (end, open, TokenClass::Comment)
            }
            Open::Str { raw_hashes } => {
                let (end, open) = literals::string(bytes, at, raw_hashes);
                (end, open, TokenClass::String)
            }
        };
        self.push(token..end, class);
        (end, open)
    }

    /// A char literal or a lifetime at the `'` at `quote`, as one token from `token`.
    fn quote(&mut self, token: usize, quote: usize) -> (usize, Open) {
        let (end, class) = literals::quote(self.line, quote);
        self.push(token..end, class);
        (end, Open::Nothing)
    }

    /// Lexes what starts at `at`, with nothing open there.
    fn step(&mut self, at: usize) -> (usize, Open) {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        match bytes[at] {
            b'/' if next == Some(b'/') => {
                self.push(at..bytes.len(), TokenClass::Comment);
                (bytes.len(), Open::Nothing)
            }
            b'/' if next == Some(b'*') => self.literal(Open::BlockComment { depth: 1 }, at, at + 2),
            b'"' => self.literal(Open::Str { raw_hashes: None }, at, at + 1),
            b'\'' => self.quote(at, at),
            byte if is_word(byte) => self.word(at),
            _ => (at + 1, Open::Nothing),
        }
    }

    /// A word at `at`: a literal's prefix, a raw identifier or a keyword.
    fn word(&mut self, at: usize) -> (usize, Open) {
        let bytes = self.line.as_bytes();
        let end = word_end(bytes, at);
        let word = &self.line[at..end];
        let after = bytes.get(end).copied();
        if matches!(word, "r" | "br" | "cr")
            && let Some((body, hashes)) = literals::raw_open(bytes, end)
        {
            self.literal(
                Open::Str {
                    raw_hashes: Some(hashes),
                },
                at,
                body,
            )
        } else if matches!(word, "b" | "c") && after == Some(b'"') {
            self.literal(Open::Str { raw_hashes: None }, at, end + 1)
        } else if word == "b" && after == Some(b'\'') {
            self.quote(at, end)
        } else if word == "r" && after == Some(b'#') {
            (word_end(bytes, end + 1), Open::Nothing)
        } else {
            if KEYWORDS.contains(&word) {
                self.push(at..end, TokenClass::Keyword);
            }
            (end, Open::Nothing)
        }
    }
}

pub(crate) fn lex(
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    let mut tokens = Tokens { line, start, out };
    let (mut at, mut open) = tokens.literal(state.open, 0, 0);
    while open == Open::Nothing && at < line.len() {
        (at, open) = tokens.step(at);
    }
    LineState { open }
}

#[cfg(test)]
mod tests;
