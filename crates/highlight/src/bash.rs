//! Bash: comments, strings, heredocs, parameter expansions, arithmetic, keywords, and commands
//! with their arguments in context (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{
    self, Comment, Constant, Function, Keyword, Operator, Punctuation, String,
};
use crate::scan::{Sink, drive, skip_spaces};
use crate::state::{LineState, Nest, Open, Prev};

mod arith;
mod expand;
mod heredoc;

/// Zed's reserved words but `in`; each is a keyword only where a command could start, and `in`
/// only after `for NAME` or `case WORD` (S3 deviation 5).
const KEYWORDS: [&str; 18] = [
    "case", "declare", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function",
    "if", "local", "select", "then", "unset", "until", "while",
];

/// Bytes that end an unquoted word.
fn is_separator(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"|&;()<>`".contains(&byte)
}

/// The look-behind past a keyword in a command position.
fn after_keyword(keyword: &str) -> Prev {
    match keyword {
        "fi" | "done" | "esac" => Prev::Command,
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
        (Prev::ForIn, "do") => Some((Keyword, Prev::Other)),
        (Prev::Test, "!" | "=" | "==" | "!=" | "=~") => Some((Operator, Prev::Test)),
        (Prev::Test, _) if test_operator => Some((Keyword, Prev::Test)),
        _ => None,
    }
}

/// The ends of the name, of its `[…]` subscript if any, and of the `=` or `+=` of an assignment
/// that starts `word`.
fn assignment(word: &str) -> Option<(usize, usize, usize)> {
    let bytes = word.as_bytes();
    let name = bytes
        .iter()
        .position(|&byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))?;
    let mut op = name;
    if bytes[name] == b'[' {
        // Braces count too, so a `${…}` in the subscript closes inside it.
        let mut depth = 0_i32;
        let close = bytes[name..].iter().position(|&byte| {
            depth += match byte {
                b'[' | b'{' => 1,
                b']' | b'}' => -1,
                _ => 0,
            };
            depth == 0
        })?;
        if bytes[name + close] != b']' {
            return None;
        }
        op = name + close + 1;
    }
    let eq = op + usize::from(bytes.get(op) == Some(&b'+'));
    (name > 0 && !bytes[0].is_ascii_digit() && bytes.get(eq) == Some(&b'=')).then_some((
        name,
        op,
        eq + 1,
    ))
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
                self.state.push(Nest::Paren);
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
                let close = skip_spaces(bytes, at + 1);
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
            _ => {
                let next = match (prev, self.state.top()) {
                    (Prev::Pattern, _) => Prev::Other,
                    (_, Some(Nest::Array { declare })) => {
                        self.state.pop();
                        if declare { Prev::Declare } else { Prev::Other }
                    }
                    (_, Some(Nest::Paren)) => {
                        self.state.pop();
                        Prev::Command
                    }
                    (Prev::Test, _) => Prev::Test,
                    _ => Prev::Command,
                };
                (1, Punctuation, next)
            }
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
        if let Some(Nest::Array { .. }) = self.state.top() {
            return self.segments(at, Some(argument(word)));
        }
        let prev = self.state.prev;
        if let Some((class, next)) = whole(prev, word) {
            self.sink.push(at..end, class);
            self.state.prev = next;
            return end;
        }
        if matches!(prev, Prev::Other | Prev::Declare)
            && let Some((name, op, value)) = assignment(word)
        {
            self.subscript(at + name..at + op);
            self.sink.push(at + op..at + value, Operator);
            if at + value == end && bytes.get(end) == Some(&b'(') {
                self.sink.push(end..end + 1, Punctuation);
                self.state.push(Nest::Array {
                    declare: prev == Prev::Declare,
                });
                return end + 1;
            }
            return self.segments(at + value, Some(argument(&word[value..])));
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

    /// An assignment's subscript at `range`, empty or `[…]`: brackets as punctuation, the
    /// parameters inside lit.
    fn subscript(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let bytes = self.line.as_bytes();
        let inside = range.start + 1..range.end - 1;
        self.sink.push(range.start..inside.start, Punctuation);
        let mut lit = inside.start;
        for at in inside.clone() {
            if at >= lit && bytes[at] == b'$' && expand::is_parameter(bytes, at) {
                lit = self.parameter(at);
            }
        }
        self.sink.push(inside.end..range.end, Punctuation);
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
    let at = tokens.resume();
    drive(line.len(), at, |at| {
        (tokens.state.open == Open::Nothing).then(|| tokens.step(at))
    });
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
