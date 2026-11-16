//! Bash strings and the parameter expansions inside words, double-quoted strings and unquoted
//! heredoc bodies.

use crate::TokenClass::{Punctuation, String};
use crate::scan::quoted;
use crate::state::Open;

use super::Tokens;

/// Whether a parameter expansion starts at the `$` at `at`: `${…}`, a name, a digit, or a
/// special parameter.
pub(super) fn is_parameter(bytes: &[u8], at: usize) -> bool {
    bytes.get(at + 1).is_some_and(|&byte| {
        byte == b'{' || byte == b'_' || byte.is_ascii_alphanumeric() || b"@*#?$!-".contains(&byte)
    })
}

impl Tokens<'_> {
    /// The parameter expansion at the `$` at `at`, which `is_parameter`: `$` or `${` and `}` are
    /// punctuation, the name inside is plain. Its end.
    pub(super) fn parameter(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes[at + 1];
        if next == b'{' {
            self.sink.push(at..at + 2, Punctuation);
            let mut depth = 0_usize;
            for (close, &byte) in bytes.iter().enumerate().skip(at + 2) {
                match byte {
                    b'{' => depth += 1,
                    b'}' if depth == 0 => {
                        self.sink.push(close..close + 1, Punctuation);
                        return close + 1;
                    }
                    b'}' => depth -= 1,
                    _ => {}
                }
            }
            return bytes.len();
        }
        self.sink.push(at..at + 1, Punctuation);
        if next == b'_' || next.is_ascii_alphabetic() {
            bytes[at + 1..]
                .iter()
                .position(|&byte| !(byte == b'_' || byte.is_ascii_alphanumeric()))
                .map_or(bytes.len(), |len| at + 1 + len)
        } else {
            at + 2
        }
    }

    /// String text from `at`, as tokens from `token`, around the parameter expansions in it, up
    /// to past `closer` or the line end. A string with a `closer` stays open past the line.
    pub(super) fn expanding(
        &mut self,
        mut token: usize,
        mut at: usize,
        closer: Option<u8>,
    ) -> usize {
        let bytes = self.line.as_bytes();
        while let Some(&byte) = bytes.get(at) {
            if byte == b'\\' {
                at += 2;
            } else if Some(byte) == closer {
                self.sink.push(token..at + 1, String);
                self.state.open = Open::Nothing;
                return at + 1;
            } else if byte == b'$' && is_parameter(bytes, at) {
                self.sink.push(token..at, String);
                at = self.parameter(at);
                token = at;
            } else {
                at += 1;
            }
        }
        self.sink.push(token..bytes.len(), String);
        if let Some(quote) = closer {
            self.state.open = Open::Quoted {
                quote,
                triple: false,
                escapes: true,
            };
        }
        bytes.len()
    }

    /// A single-quoted string from `at`, as one token from `token`, up to past its `'` or the
    /// line end, where it stays open; `escapes` for `$'…'`.
    pub(super) fn single(&mut self, token: usize, at: usize, escapes: bool) -> usize {
        let bytes = self.line.as_bytes();
        let end = quoted(bytes, at, b'\'', false, escapes);
        self.state.open = match end {
            Some(_) => Open::Nothing,
            None => Open::Quoted {
                quote: b'\'',
                triple: false,
                escapes,
            },
        };
        let end = end.unwrap_or(bytes.len());
        self.sink.push(token..end, String);
        end
    }
}

#[cfg(test)]
mod tests;
