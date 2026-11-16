//! JSON and JSONC: keys, string values, numbers, literals, and comments, block comments across
//! lines (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{self, Comment, Constant, Property, Punctuation, String};
use crate::scan::{Sink, block_comment, drive, is_word, number, quoted, skip_spaces, word_end};
use crate::state::{LineState, Open};

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    open: Open,
}

impl Tokens<'_> {
    /// Scans a block comment from `at` as one token from `token`: its end on the line.
    fn block_comment(&mut self, token: usize, at: usize) -> usize {
        let end;
        (end, self.open) = block_comment(&mut self.sink, self.line.as_bytes(), token, at, b"*/");
        end
    }

    /// Lexes what starts at `at`, with nothing open there: its end.
    fn step(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        match bytes[at] {
            b'/' if next == Some(b'/') => {
                self.sink.push(at..bytes.len(), Comment);
                bytes.len()
            }
            b'/' if next == Some(b'*') => self.block_comment(at, at + 2),
            b'"' => {
                let end = quoted(bytes, at + 1, b'"', false, true).unwrap_or(bytes.len());
                let key = bytes.get(skip_spaces(bytes, end)) == Some(&b':');
                self.sink.push(at..end, if key { Property } else { String });
                end
            }
            b'-' if next.is_some_and(|byte| byte.is_ascii_digit()) => self.number(at, at + 1),
            byte if byte.is_ascii_digit() => self.number(at, at),
            byte if is_word(byte) => {
                let end = word_end(bytes, at);
                if matches!(&self.line[at..end], "true" | "false" | "null") {
                    self.sink.push(at..end, Constant);
                }
                end
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                self.sink.push(at..at + 1, Punctuation);
                at + 1
            }
            _ => at + 1,
        }
    }

    /// The number whose digits start at `digit`, as one token from `token`.
    fn number(&mut self, token: usize, digit: usize) -> usize {
        let end = number(self.line.as_bytes(), digit);
        self.sink.push(token..end, Constant);
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
        open: Open::Nothing,
    };
    let at = match state.open {
        Open::BlockComment { .. } => tokens.block_comment(0, 0),
        _ => 0,
    };
    drive(line.len(), at, |at| {
        (tokens.open == Open::Nothing).then(|| tokens.step(at))
    });
    LineState {
        open: tokens.open,
        ..state
    }
}

#[cfg(test)]
mod tests;
