//! Rust: comments, strings, char literals, lifetimes, numbers, keywords and identifiers in
//! context (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;
use crate::scan::{Sink, is_all_caps, is_word, number, skip_spaces, word_end};
use crate::state::{LineState, Nest, Open, Prev};

mod literals;
mod nesting;

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

/// Whether `(` follows `at` on the line, past spaces and a turbofish `::<…>`.
fn is_call(bytes: &[u8], at: usize) -> bool {
    let mut at = skip_spaces(bytes, at);
    if bytes[at..].starts_with(b"::") {
        let open = skip_spaces(bytes, at + 2);
        match generic_end(bytes, open) {
            Some(end) => at = skip_spaces(bytes, end),
            None => return false,
        }
    }
    bytes.get(at) == Some(&b'(')
}

/// The end of the generic list whose `<` is at `open`, if it closes on the line; `->` closes none.
fn generic_end(bytes: &[u8], open: usize) -> Option<usize> {
    if bytes.get(open) != Some(&b'<') {
        return None;
    }
    let mut depth = 0_usize;
    for at in open..bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' if bytes[at - 1] != b'-' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// `true`, `false`, or an all-caps name.
fn is_constant(name: &str) -> bool {
    matches!(name, "true" | "false") || is_all_caps(name)
}

/// One line's tokens, pushed at the line's start offset.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
}

impl Tokens<'_> {
    fn push(&mut self, range: Range<usize>, class: TokenClass) {
        self.sink.push(range, class);
    }

    /// Scans `open` from `at` to its closer or the line end, as one token from `token`.
    fn literal(&mut self, open: Open, token: usize, at: usize) -> (usize, Open) {
        let bytes = self.line.as_bytes();
        let (end, open, class) = match open {
            Open::Nothing | Open::Quoted { .. } => return (at, Open::Nothing),
            Open::BlockComment { depth } => {
                let (end, open) = literals::block_comment(bytes, at, depth);
                (end, open, TokenClass::Comment)
            }
            Open::Str { raw_hashes } => {
                self.state.prev = Prev::Other;
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
        self.state.prev = Prev::Other;
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
            byte if byte.is_ascii_digit() => {
                let end = number(bytes, at);
                self.push(at..end, TokenClass::Constant);
                self.state.prev = Prev::Other;
                (end, Open::Nothing)
            }
            byte if is_word(byte) => self.word(at),
            byte if byte.is_ascii_whitespace() => (at + 1, Open::Nothing),
            _ => (self.symbol(at), Open::Nothing),
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
        let call = is_call(bytes, name.end);
        let path_segment = bytes[skip_spaces(bytes, name.end)..].starts_with(b"::");
        let (end, class) = if self.state.attribute.is_some() {
            (name.end, Some(TokenClass::Attribute))
        } else if keyword {
            (name.end, Some(TokenClass::Keyword))
        } else if self.state.top() == Some(Nest::Generic) && !path_segment {
            (name.end, Some(TokenClass::Type))
        } else if self.state.prev == Prev::Dot {
            let class = if call {
                TokenClass::Function
            } else {
                TokenClass::Property
            };
            (name.end, Some(class))
        } else if bytes.get(name.end) == Some(&b'!') && bytes.get(name.end + 1) != Some(&b'=') {
            (name.end + 1, Some(TokenClass::Function))
        } else {
            let class = match self.state.prev {
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
        self.state.prev = match (keyword, word) {
            (true, "fn") => Prev::Fn,
            (true, "struct" | "enum" | "trait" | "type" | "impl") => Prev::TypeKeyword,
            (false, _) if self.state.prev == Prev::Fn => Prev::FnName,
            _ if class == Some(TokenClass::Type) => Prev::Type,
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
        sink: Sink { start, out },
        state,
    };
    let (mut at, mut open) = tokens.literal(state.open, 0, 0);
    while open == Open::Nothing && at < line.len() {
        (at, open) = tokens.step(at);
    }
    LineState {
        open,
        ..tokens.state
    }
}

#[cfg(test)]
mod tests;
