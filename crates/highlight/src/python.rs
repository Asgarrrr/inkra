//! Python: comments, strings and their prefixes, triple-quoted strings across lines, numbers,
//! keywords, decorators and identifiers in context (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Operator, Property, Punctuation, String,
    Type,
};
use crate::scan::{
    Sink, dotted_end, drive, is_all_caps, is_ident_start, is_word, number, quoted, skip_spaces,
    word_end,
};
use crate::state::{LineState, Open, Prev};

/// `print` and `exec` are builtins in Python 3; `match`, `case` and `type` are `SOFT_KEYWORDS`.
const KEYWORDS: [&str; 32] = [
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield",
];

/// Keywords only where they start a statement; names everywhere else.
const SOFT_KEYWORDS: [&str; 3] = ["match", "case", "type"];

const BUILTIN_TYPES: [&str; 17] = [
    "bool",
    "bytearray",
    "bytes",
    "complex",
    "dict",
    "float",
    "frozenset",
    "frozendict",
    "int",
    "list",
    "memoryview",
    "object",
    "range",
    "set",
    "slice",
    "str",
    "tuple",
];

/// String prefixes, matched in any case.
const PREFIXES: [&str; 11] = ["r", "u", "b", "f", "t", "br", "rb", "fr", "rf", "tr", "rt"];

/// Bytes an operator run is made of; `.` lexes apart.
const OPERATORS: &[u8] = b"+-*/%@&|^~<>=!";

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
}

impl Tokens<'_> {
    /// Continues the string open at the line start, which only a triple quote leaves: where
    /// lexing goes on.
    fn resume(&mut self) -> usize {
        if let Open::Quoted { quote, triple, .. } = self.state.open {
            self.string(0, 0, quote, triple)
        } else {
            self.state.open = Open::Nothing;
            0
        }
    }

    /// Lexes what starts at `at`, with nothing open there: its end.
    fn step(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        match bytes[at] {
            b'#' => {
                self.sink.push(at..bytes.len(), Comment);
                bytes.len()
            }
            b'"' | b'\'' => self.open_string(at, at),
            b'.' if next.is_some_and(|byte| byte.is_ascii_digit()) => self.number(at),
            byte if byte.is_ascii_digit() => self.number(at),
            b'@' if next.is_some_and(is_ident_start) && skip_spaces(bytes, 0) == at => {
                self.decorator(at)
            }
            byte if is_word(byte) => self.word(at),
            byte if byte.is_ascii_whitespace() => at + 1,
            _ => self.symbol(at),
        }
    }

    fn number(&mut self, at: usize) -> usize {
        let end = number(self.line.as_bytes(), at);
        self.sink.push(at..end, Constant);
        self.state.prev = Prev::Other;
        end
    }

    /// The string whose quote is at `quote_at`, as one token from `token`: its end on the line.
    fn open_string(&mut self, token: usize, quote_at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let quote = bytes[quote_at];
        let triple = bytes[quote_at..].starts_with(&[quote; 3]);
        let body = quote_at + if triple { 3 } else { 1 };
        self.string(token, body, quote, triple)
    }

    /// Scans a string's body from `at` to its closer or the line end, as one token from `token`;
    /// only a triple-quoted one stays open past the line.
    fn string(&mut self, token: usize, at: usize, quote: u8, triple: bool) -> usize {
        let bytes = self.line.as_bytes();
        let end = quoted(bytes, at, quote, triple, true);
        self.state.open = match end {
            None if triple => Open::Quoted {
                quote,
                triple,
                escapes: true,
            },
            _ => Open::Nothing,
        };
        let end = end.unwrap_or(bytes.len());
        self.sink.push(token..end, String);
        self.state.prev = Prev::Other;
        end
    }

    /// `@` and the dotted name after it.
    fn decorator(&mut self, at: usize) -> usize {
        let end = dotted_end(self.line.as_bytes(), at + 1);
        self.sink.push(at..end, Attribute);
        self.state.prev = Prev::Other;
        end
    }

    /// A word at `at`: a string's prefix or an identifier.
    fn word(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let end = word_end(bytes, at);
        let word = &self.line[at..end];
        if matches!(bytes.get(end), Some(b'"' | b'\''))
            && PREFIXES
                .iter()
                .any(|prefix| prefix.eq_ignore_ascii_case(word))
        {
            return self.open_string(at, end);
        }
        let class = self.class(word, at, end);
        if let Some(class) = class {
            self.sink.push(at..end, class);
        }
        self.state.prev = match (class, word) {
            (Some(Keyword), "def") => Prev::Fn,
            (Some(Keyword), "class") => Prev::TypeKeyword,
            _ => Prev::Other,
        };
        end
    }

    /// The class of the identifier `word` at `at..end`.
    fn class(&self, word: &str, at: usize, end: usize) -> Option<TokenClass> {
        let bytes = self.line.as_bytes();
        let call = bytes.get(skip_spaces(bytes, end)) == Some(&b'(');
        let bare = word.trim_start_matches('_');
        if matches!(word, "True" | "False" | "None") {
            Some(Constant)
        } else if KEYWORDS.contains(&word) || self.soft_keyword(word, at, end) {
            Some(Keyword)
        } else if self.state.prev == Prev::Dot {
            Some(if call { Function } else { Property })
        } else if self.state.prev == Prev::Fn {
            Some(Function)
        } else if self.state.prev == Prev::TypeKeyword {
            Some(Type)
        } else if is_all_caps(bare) || matches!(bare.as_bytes(), [b'A'..=b'Z']) {
            Some(Constant)
        } else if bare.starts_with(|char: char| char.is_ascii_uppercase())
            || BUILTIN_TYPES.contains(&word)
        {
            Some(Type)
        } else if call {
            Some(Function)
        } else {
            None
        }
    }

    /// A soft keyword that starts its line and is followed by a space and an operand, not by
    /// what makes it a name: `=`, `.`, `:`, `,` or `)`.
    fn soft_keyword(&self, word: &str, at: usize, end: usize) -> bool {
        let bytes = self.line.as_bytes();
        SOFT_KEYWORDS.contains(&word)
            && skip_spaces(bytes, 0) == at
            && matches!(bytes.get(end), Some(b' ' | b'\t'))
            && bytes
                .get(skip_spaces(bytes, end))
                .is_some_and(|byte| !b"=.:,)".contains(byte))
    }

    /// The symbol at `at`, a byte that starts no comment, literal, word or whitespace: its end.
    fn symbol(&mut self, at: usize) -> usize {
        let rest = &self.line.as_bytes()[at..];
        let (len, class) = if rest.starts_with(b"...") {
            (3, Some(Constant))
        } else if rest.starts_with(b":=") || rest.starts_with(b"->") {
            (2, Some(Operator))
        } else {
            match rest[0] {
                b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b':' | b';' | b'.' => {
                    (1, Some(Punctuation))
                }
                byte if OPERATORS.contains(&byte) => (
                    rest.iter()
                        .take_while(|byte| OPERATORS.contains(byte))
                        .count(),
                    Some(Operator),
                ),
                _ => (1, None),
            }
        };
        if let Some(class) = class {
            self.sink.push(at..at + len, class);
        }
        self.state.prev = if &rest[..len] == b"." {
            Prev::Dot
        } else {
            Prev::Other
        };
        at + len
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
    let at = tokens.resume();
    drive(line.len(), at, |at| {
        (tokens.state.open == Open::Nothing).then(|| tokens.step(at))
    });
    tokens.state
}

#[cfg(test)]
mod tests;
