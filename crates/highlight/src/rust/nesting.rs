//! Rust punctuation, operators and attribute openers, and the nesting stack they push and pop:
//! brackets, generic lists and attributes (code highlighting spec, D2 and D4).

use crate::TokenClass::{self, Attribute, Operator, Punctuation};
use crate::state::{Nest, Prev};

use super::Tokens;

/// Bytes an operator run is made of; `<`, `>` and `.` lex apart.
const OPERATORS: &[u8] = b"+-*/%^!&|=?~@";

impl Tokens<'_> {
    /// The symbol at `at`, a byte that starts no comment, literal, word or whitespace: its end.
    pub(super) fn symbol(&mut self, at: usize) -> usize {
        let rest = &self.line.as_bytes()[at..];
        let (len, class) = if rest.starts_with(b"->") || rest.starts_with(b"=>") {
            (2, Some(Operator))
        } else if rest.starts_with(b"::") {
            (2, Some(Punctuation))
        } else {
            match rest[0] {
                b'.' => (
                    rest.iter().take_while(|&&byte| byte == b'.').count(),
                    Some(Operator),
                ),
                b'#' => self.hash(rest),
                b'(' => self.open(Nest::Paren),
                b'[' => self.open(Nest::Bracket),
                b'{' => {
                    self.drop_generics();
                    self.open(Nest::Brace)
                }
                b')' => (1, Some(self.close(Nest::Paren))),
                b']' => (1, Some(self.close(Nest::Bracket))),
                b'}' => (1, Some(self.close(Nest::Brace))),
                b'>' => (1, Some(self.close(Nest::Generic))),
                b';' => {
                    self.drop_generics();
                    (1, Some(Punctuation))
                }
                b',' | b':' => (1, Some(Punctuation)),
                b'<' if self.state.room()
                    && matches!(
                        self.state.prev,
                        Prev::Type | Prev::TypeKeyword | Prev::FnName | Prev::PathSep
                    ) =>
                {
                    self.open(Nest::Generic)
                }
                b'<' => (1, Some(Operator)),
                byte if OPERATORS.contains(&byte) => (operator_len(rest), Some(Operator)),
                _ => (1, None),
            }
        };
        let class = if self.state.attribute.is_some() {
            Some(Attribute)
        } else {
            class
        };
        if let Some(class) = class {
            self.push(at..at + len, class);
        }
        self.state.prev = match &rest[..len] {
            b"::" => Prev::PathSep,
            b"." => Prev::Dot,
            _ => Prev::Other,
        };
        at + len
    }

    /// `#[` or `#![` opens an attribute, outside one and with room on the stack; else `#` alone.
    fn hash(&mut self, rest: &[u8]) -> (usize, Option<TokenClass>) {
        let bang = usize::from(rest.get(1) == Some(&b'!'));
        if rest.get(1 + bang) == Some(&b'[') && self.state.attribute.is_none() && self.state.room()
        {
            self.state.push(Nest::Attribute);
            self.state.attribute = Some(self.state.depth);
            (2 + bang, Some(Attribute))
        } else {
            (1, None)
        }
    }

    fn open(&mut self, nest: Nest) -> (usize, Option<TokenClass>) {
        self.state.push(nest);
        (1, Some(Punctuation))
    }

    /// Pops what `closer` closes, if the top is that; every closer but `>` drops generic lists
    /// first, so a `<` taken for one does not outlive its bracket.
    fn close(&mut self, closer: Nest) -> TokenClass {
        if closer != Nest::Generic {
            self.drop_generics();
        }
        match self.state.top() {
            top if top == Some(closer) => {
                self.state.pop();
                Punctuation
            }
            Some(Nest::Attribute) if closer == Nest::Bracket => {
                self.state.pop();
                self.state.attribute = None;
                Attribute
            }
            None if closer != Nest::Generic => {
                self.state.pop();
                Punctuation
            }
            _ if closer == Nest::Generic => Operator,
            _ => Punctuation,
        }
    }

    fn drop_generics(&mut self) {
        while self.state.top() == Some(Nest::Generic) {
            self.state.pop();
        }
    }
}

/// The operator run at the start of `rest`, up to a comment's start.
fn operator_len(rest: &[u8]) -> usize {
    (0..rest.len())
        .find(|&at| !OPERATORS.contains(&rest[at]) || matches!(rest[at..], [b'/', b'/' | b'*', ..]))
        .unwrap_or(rest.len())
}

#[cfg(test)]
mod tests;
