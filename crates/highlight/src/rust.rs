//! Rust: comments, strings, char literals, lifetimes, numbers, keywords and identifiers in
//! context (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;
use crate::state::{LineState, Open, Prev};

mod literals;

/// `Self`, `true`, `false`, `union` and `macro_rules` are not keywords here.
const KEYWORDS: [&str; 35] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

const BUILTIN_TYPES: [&str; 17] = [
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
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

/// `true`, `false`, or two or more of `[A-Z0-9_]` from an ASCII uppercase letter.
fn is_constant(name: &str) -> bool {
    let bytes = name.as_bytes();
    matches!(name, "true" | "false")
        || bytes.len() >= 2
            && bytes[0].is_ascii_uppercase()
            && bytes
                .iter()
                .all(|&byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// One line's tokens, pushed at the line's start offset.
struct Tokens<'a> {
    line: &'a str,
    start: ByteOffset,
    out: &'a mut Vec<(Range<ByteOffset>, TokenClass)>,
    prev: Prev,
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
                self.prev = Prev::Other;
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
        self.prev = Prev::Other;
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
            b'.' => {
                let end = at + bytes[at..].iter().take_while(|&&byte| byte == b'.').count();
                self.prev = if end - at == 1 {
                    Prev::Dot
                } else {
                    Prev::Other
                };
                (end, Open::Nothing)
            }
            byte if byte.is_ascii_digit() => {
                let end = literals::number(bytes, at);
                self.push(at..end, TokenClass::Constant);
                self.prev = Prev::Other;
                (end, Open::Nothing)
            }
            byte if is_word(byte) => self.word(at),
            byte if byte.is_ascii_whitespace() => (at + 1, Open::Nothing),
            _ => {
                self.prev = Prev::Other;
                (at + 1, Open::Nothing)
            }
        }
    }

    /// A word at `at`: a literal's prefix or an identifier.
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
            (
                self.identifier(at, end + 1..word_end(bytes, end + 1)),
                Open::Nothing,
            )
        } else {
            (self.identifier(at, at..end), Open::Nothing)
        }
    }

    /// The identifier at `name`, raw when its token starts at `token` before it: its token's end.
    fn identifier(&mut self, token: usize, name: Range<usize>) -> usize {
        let bytes = self.line.as_bytes();
        let word = &self.line[name.start..name.end];
        let keyword = token == name.start && KEYWORDS.contains(&word);
        let call = bytes[name.end..]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(&b'(');
        let (end, class) = if keyword {
            (name.end, Some(TokenClass::Keyword))
        } else if self.prev == Prev::Dot {
            let class = if call {
                TokenClass::Function
            } else {
                TokenClass::Property
            };
            (name.end, Some(class))
        } else if bytes.get(name.end) == Some(&b'!') && bytes.get(name.end + 1) != Some(&b'=') {
            (name.end + 1, Some(TokenClass::Function))
        } else {
            let class = match self.prev {
                Prev::Fn => Some(TokenClass::Function),
                Prev::TypeKeyword => Some(TokenClass::Type),
                _ if is_constant(word) => Some(TokenClass::Constant),
                _ if word.starts_with(|char: char| char.is_ascii_uppercase())
                    || BUILTIN_TYPES.contains(&word) =>
                {
                    Some(TokenClass::Type)
                }
                _ if call => Some(TokenClass::Function),
                _ => None,
            };
            (name.end, class)
        };
        if let Some(class) = class
            && !name.is_empty()
        {
            self.push(token..end, class);
        }
        self.prev = match (keyword, word) {
            (true, "fn") => Prev::Fn,
            (true, "struct" | "enum" | "trait" | "type" | "impl") => Prev::TypeKeyword,
            _ => Prev::Other,
        };
        end
    }
}

pub(crate) fn lex(
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    let mut tokens = Tokens {
        line,
        start,
        out,
        prev: state.prev,
    };
    let (mut at, mut open) = tokens.literal(state.open, 0, 0);
    while open == Open::Nothing && at < line.len() {
        (at, open) = tokens.step(at);
    }
    LineState {
        open,
        prev: tokens.prev,
    }
}

#[cfg(test)]
mod tests;
