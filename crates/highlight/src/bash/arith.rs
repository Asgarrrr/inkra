//! Bash arithmetic inside `((` and `$((`: numbers, operators, plain names, nested parentheses.

use crate::TokenClass::{Constant, Operator, Punctuation};
use crate::scan::{is_word, number, word_end};
use crate::state::{Nest, Prev};

use super::{Tokens, expand};

/// Bytes an arithmetic operator run is made of.
const ARITH_OPERATORS: &[u8] = b"+-*/%<>=!&|^~?:";

impl Tokens<'_> {
    pub(super) fn in_arith(&self) -> bool {
        let stored = usize::from(self.state.depth).min(self.state.stack.len());
        self.state.stack[..stored].contains(&Nest::Arith)
    }

    pub(super) fn open_arith(&mut self, at: usize, len: usize) -> usize {
        self.sink.push(at..at + len, Punctuation);
        self.state.push(Nest::Arith);
        at + len
    }

    /// Lexes what starts at `at` inside `((`: names are plain, `))` closes it.
    pub(super) fn arith(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let rest = &bytes[at..];
        let top = self.state.top();
        let (len, class) = match rest[0] {
            b')' if top == Some(Nest::Arith) && rest.starts_with(b"))") => {
                self.state.pop();
                self.state.prev = Prev::Command;
                (2, Some(Punctuation))
            }
            b')' => {
                if top != Some(Nest::Arith) {
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
}

#[cfg(test)]
mod tests;
