//! CSS: selectors, properties, values with their units and colours, at-rules, strings, and
//! comments across lines (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{
    self, Attribute, Constant, Function, Keyword, Operator, Property, Punctuation, String, Tag,
    Type,
};
use crate::scan::{
    Sink, block_comment, drive, end_while, is_ident_start, is_word, quoted, skip_spaces,
};
use crate::state::{LineState, Nest, Open, Prev};

/// Whether a name starts at `at`: a letter, `_` or non-ASCII, or a `-` before one or another `-`.
fn starts_name(bytes: &[u8], at: usize) -> bool {
    match bytes.get(at) {
        Some(b'-') => bytes
            .get(at + 1)
            .is_some_and(|&byte| is_ident_start(byte) || byte == b'-'),
        Some(&byte) => is_ident_start(byte),
        None => false,
    }
}

/// A byte of a name after its start: a word byte or `-`.
fn is_name(byte: u8) -> bool {
    is_word(byte) || byte == b'-'
}

/// The ends of the number at `at`, its sign included, and of its unit, `%` or letters; `None`
/// without a digit.
fn number(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let digits = |from: usize| {
        bytes.get(from..).map_or(0, |rest| {
            rest.iter().take_while(|byte| byte.is_ascii_digit()).count()
        })
    };
    let mut end = at + usize::from(matches!(bytes[at], b'+' | b'-'));
    let whole = digits(end);
    end += whole;
    let fraction = if bytes.get(end) == Some(&b'.') {
        digits(end + 1)
    } else {
        0
    };
    if whole + fraction == 0 {
        return None;
    }
    if fraction > 0 {
        end += 1 + fraction;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    let unit = if bytes.get(end) == Some(&b'%') {
        end + 1
    } else {
        end + bytes[end..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphabetic())
            .count()
    };
    Some((end, unit))
}

/// Whether the item at `at` is a declaration, not a selector: the first `{`, `;` or `}` after it
/// on its line is no `{`; with none, its name is followed by `:` and a space or the line end.
fn declaration_ahead(bytes: &[u8], at: usize) -> bool {
    let mut scan = at;
    while let Some(&byte) = bytes.get(scan) {
        match byte {
            b'{' => return false,
            b';' | b'}' => return true,
            b'"' | b'\'' => {
                scan = quoted(bytes, scan + 1, byte, false, true).unwrap_or(bytes.len());
            }
            _ => scan += 1,
        }
    }
    let colon = skip_spaces(bytes, end_while(bytes, at, is_name));
    bytes.get(colon) == Some(&b':') && bytes.get(colon + 1).is_none_or(u8::is_ascii_whitespace)
}

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
}

impl Tokens<'_> {
    fn block_comment(&mut self, token: usize, at: usize) -> usize {
        let end;
        (end, self.state.open) =
            block_comment(&mut self.sink, self.line.as_bytes(), token, at, b"*/");
        end
    }

    fn punctuation(&mut self, at: usize) -> usize {
        self.sink.token(at..at + 1, Punctuation)
    }

    fn open(&mut self, at: usize, entry: Nest) -> usize {
        self.state.push(entry);
        self.punctuation(at)
    }

    /// Pops `entry` if the top is that, or unknown past the stack's room.
    fn close(&mut self, at: usize, entry: Nest) -> usize {
        if self.state.top().is_none_or(|top| top == entry) {
            self.state.pop();
        }
        self.punctuation(at)
    }

    /// Lexes what starts at `at`, with nothing open there, by the look-behind: its end.
    fn step(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at + 1,
            b'/' if bytes.get(at + 1) == Some(&b'*') => self.block_comment(at, at + 2),
            quote @ (b'"' | b'\'') => {
                let end = quoted(bytes, at + 1, quote, false, true).unwrap_or(bytes.len());
                self.sink.push(at..end, String);
                end
            }
            b'{' => {
                self.state.prev = Prev::Other;
                self.open(at, Nest::Brace)
            }
            b'}' => {
                self.state.prev = Prev::Other;
                self.close(at, Nest::Brace)
            }
            b';' => {
                self.state.prev = Prev::Other;
                self.punctuation(at)
            }
            b'@' if starts_name(bytes, at + 1) => {
                let end = end_while(bytes, at + 1, is_name);
                self.sink.push(at..end, Keyword);
                self.state.prev = Prev::AtRule;
                end
            }
            _ => match self.state.prev {
                Prev::Declaration => self.value(at),
                Prev::AtRule => self.at_rule(at),
                Prev::Selector => self.selector(at),
                _ if starts_name(bytes, at) && declaration_ahead(bytes, at) => {
                    let end = end_while(bytes, at, is_name);
                    if !bytes[at..].starts_with(b"--") {
                        self.sink.push(at..end, Property);
                    }
                    self.state.prev = Prev::Declaration;
                    end
                }
                _ => {
                    self.state.prev = Prev::Selector;
                    self.selector(at)
                }
            },
        }
    }

    /// The number at `at`, a `Constant`, and its unit, a `Type`, as Zed colours `unit`.
    fn number(&mut self, at: usize) -> Option<usize> {
        let (end, unit) = number(self.line.as_bytes(), at)?;
        self.sink.push(at..end, Constant);
        self.sink.push(end..unit, Type);
        Some(unit)
    }

    /// The name at `at` called as a function: its name and, for `url(` without quotes, its raw
    /// argument up to `)`.
    fn function(&mut self, at: usize, end: usize) -> usize {
        let bytes = self.line.as_bytes();
        self.sink.push(at..end, Function);
        let open = end + 1;
        if !self.line[at..end].eq_ignore_ascii_case("url")
            || matches!(bytes.get(skip_spaces(bytes, open)), Some(b'"' | b'\''))
        {
            return end;
        }
        self.sink.push(end..open, Punctuation);
        let Some(close) = bytes[open..].iter().position(|&byte| byte == b')') else {
            self.sink.push(open..bytes.len(), Constant);
            return bytes.len();
        };
        self.sink.push(open..open + close, Constant);
        self.punctuation(open + close)
    }

    /// What starts at `at` in a declaration's value.
    fn value(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        if let Some(end) = self.number(at) {
            return end;
        }
        match bytes[at] {
            b'#' if bytes.get(at + 1).is_some_and(|&byte| is_word(byte)) => {
                let end = end_while(bytes, at + 1, is_name);
                self.sink.push(at..end, String);
                end
            }
            b'!' => {
                let word = skip_spaces(bytes, at + 1);
                let end = end_while(bytes, word, is_name);
                let important = self.line[word..end].eq_ignore_ascii_case("important");
                let end = if important { end } else { at + 1 };
                self.sink
                    .push(at..end, if important { Keyword } else { Operator });
                end
            }
            b'(' => self.open(at, Nest::Paren),
            b')' => self.close(at, Nest::Paren),
            b',' | b':' => self.punctuation(at),
            _ if starts_name(bytes, at) => {
                let end = end_while(bytes, at, is_name);
                if bytes.get(end) == Some(&b'(') {
                    self.function(at, end)
                } else {
                    // A custom property is a variable, plain as in Zed.
                    if !bytes[at..].starts_with(b"--") {
                        self.sink.push(at..end, Constant);
                    }
                    end
                }
            }
            b'/' | b'*' | b'+' | b'-' | b'=' | b'<' | b'>' => {
                self.sink.push(at..at + 1, Operator);
                at + 1
            }
            _ => at + 1,
        }
    }

    /// What starts at `at` in an at-rule's prelude: media queries, a keyframes name, imports.
    fn at_rule(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        if let Some(end) = self.number(at) {
            return end;
        }
        match bytes[at] {
            b'(' => self.open(at, Nest::Paren),
            b')' => self.close(at, Nest::Paren),
            b',' | b':' => self.punctuation(at),
            _ if starts_name(bytes, at) => {
                let end = end_while(bytes, at, is_name);
                let word = &self.line[at..end];
                let feature = self.state.top() == Some(Nest::Paren)
                    && bytes.get(skip_spaces(bytes, end)) == Some(&b':');
                let class = match word {
                    "and" | "or" | "not" | "only" => Keyword,
                    _ if bytes.get(end) == Some(&b'(') => return self.function(at, end),
                    _ if feature => Property,
                    _ => Constant,
                };
                self.sink.push(at..end, class);
                end
            }
            b'=' | b'<' | b'>' => {
                self.sink.push(at..at + 1, Operator);
                at + 1
            }
            _ => at + 1,
        }
    }

    /// What starts at `at` in a selector.
    fn selector(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        if bytes[at] == b']' {
            return self.close(at, Nest::Bracket);
        }
        if self.state.top() == Some(Nest::Bracket) {
            return self.attribute_selector(at);
        }
        if let Some(end) = self.number(at) {
            return end;
        }
        match bytes[at] {
            // Zed has no class for `selector.class`, `selector.id` or `selector.pseudo`.
            b'.' | b'#' if starts_name(bytes, at + 1) => {
                self.punctuation(at);
                end_while(bytes, at + 1, is_name)
            }
            b':' => {
                let colons = if next == Some(b':') { 2 } else { 1 };
                self.sink.push(at..at + colons, Punctuation);
                if starts_name(bytes, at + colons) {
                    end_while(bytes, at + colons, is_name)
                } else {
                    at + colons
                }
            }
            b'*' | b'&' => {
                self.sink.push(at..at + 1, Tag);
                at + 1
            }
            b'[' => self.open(at, Nest::Bracket),
            b'(' => self.open(at, Nest::Paren),
            b')' => self.close(at, Nest::Paren),
            b',' => self.punctuation(at),
            b'>' | b'+' | b'~' | b'|' => {
                self.sink.push(at..at + 1, Operator);
                at + 1
            }
            _ if starts_name(bytes, at) => {
                let end = end_while(bytes, at, is_name);
                let keyframe = matches!(&self.line[at..end], "from" | "to");
                self.sink
                    .push(at..end, if keyframe { Keyword } else { Tag });
                end
            }
            _ => at + 1,
        }
    }

    /// What starts at `at` inside `[…]`: the attribute's name, its operator, an unquoted value.
    fn attribute_selector(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        if starts_name(bytes, at) {
            let end = end_while(bytes, at, is_name);
            let before = bytes[..at]
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace());
            let value = before.is_some_and(|before| bytes[before] == b'=');
            self.sink
                .push(at..end, if value { String } else { Attribute });
            return end;
        }
        let len = match &bytes[at..] {
            [b'~' | b'|' | b'^' | b'$' | b'*', b'=', ..] => 2,
            [b'=', ..] => 1,
            _ => return at + 1,
        };
        self.sink.push(at..at + len, Operator);
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
    let at = if let Open::BlockComment { .. } = state.open {
        tokens.block_comment(0, 0)
    } else {
        tokens.state.open = Open::Nothing;
        0
    };
    drive(line.len(), at, |at| {
        (tokens.state.open == Open::Nothing).then(|| tokens.step(at))
    });
    tokens.state
}

#[cfg(test)]
mod tests;
