//! Bash: comments, strings, heredocs, parameter expansions, arithmetic, keywords, and commands
//! with their arguments in context (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{
    self, Comment, Constant, Function, Keyword, Operator, Punctuation, String,
};
use crate::scan::{Sink, is_word, number, word_end};
use crate::state::{LineState, Nest, Open, Prev};

mod expand;
mod heredoc;

/// Zed's reserved words; each is a keyword only where a command could start.
const KEYWORDS: [&str; 19] = [
    "case", "declare", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function",
    "if", "in", "local", "select", "then", "unset", "until", "while",
];

/// Bytes an arithmetic operator run is made of.
const ARITH_OPERATORS: &[u8] = b"+-*/%<>=!&|^~?:";

/// Bytes that end an unquoted word.
fn is_separator(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"|&;()<>`".contains(&byte)
}

/// The look-behind past a keyword in a command position.
fn after_keyword(keyword: &str) -> Prev {
    match keyword {
        "fi" | "done" | "esac" | "in" => Prev::Command,
        "for" | "select" => Prev::For,
        "case" => Prev::Case,
        "function" => Prev::Fn,
        "export" | "unset" | "local" | "declare" => Prev::Declare,
        _ => Prev::Other,
    }
}

/// A word that is one token whole, by the look-behind: its class and the look-behind after it.
fn whole(prev: Prev, word: &str) -> Option<(TokenClass, Prev)> {
    let test_operator = word.len() > 1
        && word.starts_with('-')
        && word[1..].bytes().all(|b| b.is_ascii_alphabetic());
    match (prev, word) {
        (Prev::Other, "[" | "[[") => Some((Punctuation, Prev::Test)),
        (Prev::Other, "{") => Some((Punctuation, Prev::Other)),
        (Prev::Other, "}") | (Prev::Test, "]" | "]]") => Some((Punctuation, Prev::Command)),
        (Prev::Other, "!") => Some((Operator, Prev::Other)),
        (Prev::Other, _) if KEYWORDS.contains(&word) => Some((Keyword, after_keyword(word))),
        (Prev::ForIn, "in") | (Prev::Pattern, "esac") => Some((Keyword, Prev::Command)),
        (Prev::CaseIn, "in") => Some((Keyword, Prev::Pattern)),
        (Prev::Test, "!" | "=" | "==" | "!=" | "=~") => Some((Operator, Prev::Test)),
        (Prev::Test, _) if test_operator => Some((Keyword, Prev::Test)),
        _ => None,
    }
}

/// The ends of the name and of the `=` or `+=` of an assignment that starts `word`.
fn assignment(word: &str) -> Option<(usize, usize)> {
    let bytes = word.as_bytes();
    let name = bytes
        .iter()
        .position(|&byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))?;
    let op = name + usize::from(bytes[name] == b'+');
    (name > 0 && !bytes[0].is_ascii_digit() && bytes.get(op) == Some(&b'='))
        .then_some((name, op + 1))
}

/// The class of an argument's unquoted text: a flag or a number is a constant.
fn argument(word: &str) -> TokenClass {
    if word.starts_with('-') || !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
        Constant
    } else {
        String
    }
}

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
    /// A comment ran to the line end, so a last `\` continues nothing.
    comment: bool,
}

impl Tokens<'_> {
    /// Continues the string open at the line start: where lexing goes on.
    fn resume(&mut self) -> usize {
        match self.state.open {
            Open::Quoted { quote: b'"', .. } => self.expanding(0, 0, Some(b'"')),
            Open::Quoted { escapes, .. } => self.single(0, 0, escapes),
            _ => {
                self.state.open = Open::Nothing;
                0
            }
        }
    }

    fn in_arith(&self) -> bool {
        let stored = usize::from(self.state.depth).min(self.state.stack.len());
        self.state.stack[..stored].contains(&Nest::Arith)
    }

    /// Lexes what starts at `at`, with nothing open there: its end.
    fn step(&mut self, at: usize) -> usize {
        if self.in_arith() {
            return self.arith(at);
        }
        let bytes = self.line.as_bytes();
        let rest = &bytes[at..];
        match rest[0] {
            byte if byte.is_ascii_whitespace() => at + 1,
            b'#' => {
                self.sink.push(at..bytes.len(), Comment);
                self.comment = true;
                bytes.len()
            }
            b'\\' if rest.len() == 1 => at + 1,
            b'`' => {
                self.sink.push(at..at + 1, Punctuation);
                if self.state.top() == Some(Nest::Backtick) {
                    self.state.pop();
                    self.state.prev = Prev::Command;
                } else {
                    self.state.push(Nest::Backtick);
                    self.state.prev = Prev::Other;
                }
                at + 1
            }
            b'$' if rest.starts_with(b"$((") => self.open_arith(at, 3),
            b'$' if rest.starts_with(b"$(") => {
                self.sink.push(at..at + 2, Punctuation);
                self.state.prev = Prev::Other;
                at + 2
            }
            b'(' if rest.starts_with(b"((")
                && matches!(self.state.prev, Prev::Other | Prev::For) =>
            {
                self.open_arith(at, 2)
            }
            b'(' | b')' | b';' | b'|' | b'&' | b'<' | b'>' => self.symbol(at),
            _ => self.word(at),
        }
    }

    fn open_arith(&mut self, at: usize, len: usize) -> usize {
        self.sink.push(at..at + len, Punctuation);
        self.state.push(Nest::Arith);
        at + len
    }

    /// Lexes what starts at `at` inside `((`: names are plain, `))` closes it.
    fn arith(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let rest = &bytes[at..];
        let top = self.state.top();
        let (len, class) = match rest[0] {
            byte if byte.is_ascii_whitespace() => (1, None),
            b')' if top == Some(Nest::Arith) && rest.starts_with(b"))") => {
                self.state.pop();
                self.state.prev = Prev::Command;
                (2, Some(Punctuation))
            }
            b')' => {
                if top != Some(Nest::Arith) && self.state.depth > 0 {
                    self.state.pop();
                }
                (1, Some(Punctuation))
            }
            b'(' => {
                self.state.push(Nest::Paren);
                (1, Some(Punctuation))
            }
            b'$' if expand::is_parameter(bytes, at) => return self.parameter(at),
            b'"' => return self.expanding(at, at + 1, Some(b'"')),
            b'\'' => return self.single(at, at + 1, false),
            byte if byte.is_ascii_digit() => (number(bytes, at) - at, Some(Constant)),
            b',' | b';' => (1, Some(Punctuation)),
            byte if ARITH_OPERATORS.contains(&byte) => (
                rest.iter()
                    .take_while(|byte| ARITH_OPERATORS.contains(byte))
                    .count(),
                Some(Operator),
            ),
            byte if is_word(byte) => (word_end(bytes, at) - at, None),
            _ => (1, None),
        };
        if let Some(class) = class {
            self.sink.push(at..at + len, class);
        }
        at + len
    }

    /// An operator or punctuation at `at`, a byte that `is_separator` and is no whitespace.
    fn symbol(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let prev = self.state.prev;
        let keep = |kept: Prev| if prev == kept { kept } else { Prev::Other };
        let (len, class, next) = match &bytes[at..] {
            [b';', b';', b'&', ..] => (3, Punctuation, Prev::Pattern),
            [b';', b';' | b'&', ..] => (2, Punctuation, Prev::Pattern),
            [b';', ..] => (1, Punctuation, Prev::Other),
            [b'|', b'|', ..] | [b'&', b'&', ..] => (2, Operator, keep(Prev::Test)),
            [b'|', b'&', ..] => (2, Operator, Prev::Other),
            [b'|', ..] => (1, Operator, keep(Prev::Pattern)),
            [b'&', b'>', b'>', ..] | [b'<', b'<', b'<', ..] => (3, Operator, prev),
            [b'<', b'<', ..] => return self.heredoc(at),
            [b'&' | b'<' | b'>', b'>', ..] | [b'<' | b'>', b'&' | b'|', ..] => (2, Operator, prev),
            [b'&', ..] => (1, Operator, Prev::Other),
            [b'<' | b'>', ..] => (1, Operator, prev),
            [b'(', ..] => {
                let close = crate::scan::skip_spaces(bytes, at + 1);
                if bytes.get(close) == Some(&b')') && !matches!(prev, Prev::Pattern | Prev::Test) {
                    self.sink.push(at..at + 1, Punctuation);
                    self.sink.push(close..close + 1, Punctuation);
                    self.state.prev = Prev::Other;
                    return close + 1;
                }
                (
                    1,
                    Punctuation,
                    if matches!(prev, Prev::Pattern | Prev::Test) {
                        prev
                    } else {
                        Prev::Other
                    },
                )
            }
            _ => (
                1,
                Punctuation,
                match prev {
                    Prev::Pattern => Prev::Other,
                    Prev::Test => Prev::Test,
                    _ => Prev::Command,
                },
            ),
        };
        self.sink.push(at..at + len, class);
        self.state.prev = next;
        at + len
    }

    /// A word at `at`: a keyword, an assignment, a command's name or an argument.
    fn word(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let end = bytes[at..]
            .iter()
            .position(|&byte| is_separator(byte))
            .map_or(bytes.len(), |len| at + len);
        let word = &self.line[at..end];
        let prev = self.state.prev;
        if let Some((class, next)) = whole(prev, word) {
            self.sink.push(at..end, class);
            self.state.prev = next;
            return end;
        }
        if matches!(prev, Prev::Other | Prev::Declare)
            && let Some((name, op)) = assignment(word)
        {
            self.sink.push(at + name..at + op, Operator);
            return self.segments(at + op, Some(argument(&word[op..])));
        }
        let (class, next) = match prev {
            Prev::Other => (Some(Function), Prev::Command),
            Prev::Fn => (Some(Function), Prev::Other),
            Prev::Declare => (word.starts_with('-').then_some(Constant), Prev::Declare),
            Prev::For => (None, Prev::ForIn),
            Prev::Case => (Some(argument(word)), Prev::CaseIn),
            Prev::Pattern => (Some(String), Prev::Pattern),
            Prev::Test => (Some(argument(word)), Prev::Test),
            _ => (Some(argument(word)), Prev::Command),
        };
        self.state.prev = next;
        self.segments(at, class)
    }

    /// The pieces of the word at `at` up to its end or an open string: unquoted text in `class`,
    /// strings and expansions in theirs. A `$(` ends it, for `step` to open.
    fn segments(&mut self, mut at: usize, class: Option<TokenClass>) -> usize {
        let bytes = self.line.as_bytes();
        while let Some(&byte) = bytes.get(at) {
            if is_separator(byte) || self.state.open != Open::Nothing {
                break;
            }
            at = match (byte, bytes.get(at + 1)) {
                (b'"', _) => self.expanding(at, at + 1, Some(b'"')),
                (b'\'', _) => self.single(at, at + 1, false),
                (b'$', Some(b'(')) => break,
                (b'$', Some(b'\'')) => self.single(at, at + 2, true),
                (b'$', Some(b'"')) => self.expanding(at, at + 2, Some(b'"')),
                (b'$', _) if expand::is_parameter(bytes, at) => self.parameter(at),
                _ => self.unquoted(at, class),
            };
        }
        at
    }

    /// Unquoted text from `at`, its first byte included, up to a separator, a quote or a `$`.
    fn unquoted(&mut self, at: usize, class: Option<TokenClass>) -> usize {
        let bytes = self.line.as_bytes();
        let mut end = at;
        loop {
            match bytes.get(end) {
                Some(b'\\') => end = (end + 2).min(bytes.len()),
                Some(&byte)
                    if end == at
                        || !(is_separator(byte) || matches!(byte, b'"' | b'\'' | b'$')) =>
                {
                    end += 1;
                }
                _ => break,
            }
        }
        if let Some(class) = class {
            self.sink.push(at..end, class);
        }
        end
    }
}

/// Whether the line ends in a `\` that escapes its newline.
fn continues(line: &str) -> bool {
    line.bytes().rev().take_while(|&byte| byte == b'\\').count() % 2 == 1
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
        comment: false,
    };
    if let (Open::Nothing, Some(heredoc)) = (state.open, state.heredocs[0]) {
        tokens.body(heredoc);
        return tokens.state;
    }
    let mut at = tokens.resume();
    while tokens.state.open == Open::Nothing && at < line.len() {
        at = tokens.step(at);
    }
    if tokens.state.open == Open::Nothing && (tokens.comment || !continues(line)) {
        tokens.state.prev = match tokens.state.prev {
            Prev::Command | Prev::Declare | Prev::Fn | Prev::Test => Prev::Other,
            prev => prev,
        };
    }
    tokens.state
}

#[cfg(test)]
mod tests;
