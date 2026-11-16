//! JavaScript punctuation and operators, `<` by D4's expression start, and the nesting stack
//! they push and pop: brackets, conditions and TypeScript's generic lists (code highlighting
//! spec, D2 and D4).

use crate::TokenClass::{self, Operator, Punctuation};
use crate::scan::{end_while, is_ident_start, skip_spaces};
use crate::state::{Nest, Prev};

use super::{Tokens, is_ident, is_list};

/// Longest first, so the first that matches is the longest.
const OPERATORS: [&str; 46] = [
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "**",
    "<<", ">>", "+", "-", "*", "/", "%", "<", ">", "=", "!", "&", "|", "^", "~", "?",
];

impl Tokens<'_> {
    /// A `<` at `at`: a JSX tag or a generic list where D4 and the dialect open one, else an
    /// operator.
    pub(super) fn angle(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        let letter = next.is_some_and(is_ident_start);
        let room = self.state.room();
        let generic = if self.value_end() {
            matches!(
                self.state.prev,
                Prev::Type | Prev::TypePosition | Prev::FnName
            )
        } else if self.state.prev == Prev::Annotation {
            letter
        } else if self.dialect.jsx
            && room
            && (letter || next == Some(b'>'))
            && !self.type_parameters(at)
        {
            return self.jsx_open(at);
        } else {
            letter
        };
        if self.dialect.types
            && room
            && generic
            && (self.generics.end(at).is_some() || self.continues_on_next_line(at))
        {
            let list = match self.state.prev {
                Prev::TypePosition => Nest::TypeGeneric,
                Prev::FnName => Nest::FnGeneric,
                _ => Nest::Generic,
            };
            self.state.push(list);
            self.sink.push(at..at + 1, Punctuation);
            self.state.prev = Prev::Other;
            return at + 1;
        }
        self.symbol(at)
    }

    /// Whether the `<` at `at`, after a type position or a declared function's name, opens a list
    /// its line leaves open: the line ends on `<` or `,` with no `;`, `{` or `}` past it (S4
    /// deviation 18).
    fn continues_on_next_line(&self, at: usize) -> bool {
        let rest = self.line[at..].trim_ascii_end().as_bytes();
        matches!(self.state.prev, Prev::TypePosition | Prev::FnName)
            && matches!(rest.last(), Some(b'<' | b','))
            && !rest.iter().any(|byte| matches!(byte, b';' | b'{' | b'}'))
    }

    /// Whether the `<` at `at` opens TypeScript type parameters, which JSX would read as a tag:
    /// `<T,` or `<T extends`.
    fn type_parameters(&self, at: usize) -> bool {
        let bytes = self.line.as_bytes();
        let after = skip_spaces(bytes, end_while(bytes, at + 1, is_ident));
        self.dialect.types
            && (bytes.get(after) == Some(&b',') || bytes[after..].starts_with(b"extends "))
    }

    /// The punctuation or operator at `at`, a byte that starts no comment, literal, word,
    /// whitespace or tag: its end.
    pub(super) fn symbol(&mut self, at: usize) -> usize {
        let rest = &self.line.as_bytes()[at..];
        let prev = self.state.prev;
        let (len, class, next) = match rest {
            [b'(', ..] if prev == Prev::Condition => self.open(Nest::Condition, Prev::Other),
            [b'(', ..] if self.dialect.types && matches!(prev, Prev::Fn | Prev::FnName) => {
                self.open(Nest::Params, Prev::KeyStart)
            }
            [b'(', ..] => self.open(Nest::Paren, Prev::KeyStart),
            [b'[', ..] => self.open(Nest::Bracket, Prev::Other),
            [b'{', ..] => {
                self.drop_generics();
                // Right after its `:`, a return type's `{` opens an object type; else the body.
                let mut body_start = false;
                if prev != Prev::Annotation {
                    body_start = self.end_return_type();
                }
                let body = !body_start
                    && self.dialect.types
                    && matches!(prev, Prev::TypePosition | Prev::Annotation);
                let entry = if body { Nest::TypeBody } else { Nest::Brace };
                self.open(entry, Prev::KeyStart)
            }
            [b')', ..] => {
                let next = match self.close(Nest::Paren) {
                    Some(Nest::Condition) => Prev::Other,
                    Some(Nest::Params) => Prev::Returns,
                    _ => Prev::ValueEnd,
                };
                (1, Some(Punctuation), next)
            }
            [b']', ..] => {
                self.close(Nest::Bracket);
                (1, Some(Punctuation), Prev::ValueEnd)
            }
            [b'}', ..] => {
                self.close(Nest::Brace);
                let member = self.state.top() == Some(Nest::TypeBody);
                let next = if member { Prev::KeyStart } else { Prev::Other };
                (1, Some(Punctuation), next)
            }
            [b';', ..] => {
                self.drop_generics();
                self.end_return_type();
                (1, Some(Punctuation), Prev::KeyStart)
            }
            [b',', ..] => {
                self.end_return_type();
                (1, Some(Punctuation), Prev::KeyStart)
            }
            [b'=', ..] if self.state.top() == Some(Nest::Alias) => {
                self.state.pop();
                (1, Some(Operator), Prev::Annotation)
            }
            [b':', ..] if prev == Prev::Param => (1, Some(Punctuation), Prev::Annotation),
            [b':', ..] if prev == Prev::Returns => self.open(Nest::ReturnType, Prev::Annotation),
            [b':', ..] => (1, Some(Punctuation), Prev::Other),
            [b'?', b'.', after @ ..] if !after.first().is_some_and(u8::is_ascii_digit) => {
                (2, Some(Punctuation), Prev::Dot)
            }
            [b'?', ..] if prev == Prev::Param => (1, Some(Punctuation), Prev::Param),
            [b'.', b'.', b'.', ..] => (3, Some(Operator), Prev::Other),
            [b'.', ..] => (1, Some(Punctuation), Prev::Dot),
            [b'>', ..] if is_list(self.state.top()) => {
                let next = match self.state.top() {
                    Some(Nest::TypeGeneric) => Prev::TypePosition,
                    Some(Nest::FnGeneric) => Prev::FnName,
                    _ => Prev::ValueEnd,
                };
                self.state.pop();
                (1, Some(Punctuation), next)
            }
            _ => match OPERATORS.iter().find(|op| rest.starts_with(op.as_bytes())) {
                // A postfix `++` or `--` ends a value; a prefix one starts an expression.
                Some(&op @ ("++" | "--")) if self.value_end() => {
                    (op.len(), Some(Operator), Prev::ValueEnd)
                }
                Some(op) => (op.len(), Some(Operator), Prev::Other),
                None => (1, None, Prev::Other),
            },
        };
        if let Some(class) = class {
            self.sink.push(at..at + len, class);
        }
        self.state.prev = next;
        at + len
    }

    fn open(&mut self, entry: Nest, past: Prev) -> (usize, Option<TokenClass>, Prev) {
        self.state.push(entry);
        (1, Some(Punctuation), past)
    }

    /// Pops what `closer` closes, if the top is that, after any generic list: the entry it
    /// popped. A `)` closes a `Condition` or `Params` too, and a `}` a `TypeBody`.
    fn close(&mut self, closer: Nest) -> Option<Nest> {
        self.drop_generics();
        self.end_return_type();
        let top = self.state.top();
        let pops = match top {
            Some(Nest::Condition | Nest::Params) => closer == Nest::Paren,
            Some(Nest::TypeBody) => closer == Nest::Brace,
            Some(top) => top == closer,
            None => true,
        };
        if pops {
            self.state.pop();
        }
        top.filter(|_| pops)
    }

    fn drop_generics(&mut self) {
        while is_list(self.state.top()) {
            self.state.pop();
        }
    }

    /// Pops a `ReturnType` on top: whether there was one.
    fn end_return_type(&mut self) -> bool {
        let open = self.state.top() == Some(Nest::ReturnType);
        if open {
            self.state.pop();
        }
        open
    }
}
