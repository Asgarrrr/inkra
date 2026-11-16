//! JavaScript punctuation and operators, `<` by D4's expression start, and the nesting stack
//! they push and pop: brackets, conditions and TypeScript's generic lists (code highlighting
//! spec, D2 and D4).

use crate::TokenClass::{self, Operator, Punctuation};
use crate::scan::is_ident_start;
use crate::state::{Nest, Prev};

use super::Tokens;

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
            matches!(self.state.prev, Prev::Type | Prev::FnName)
        } else if self.dialect.jsx && room && (letter || next == Some(b'>')) {
            return self.jsx_open(at);
        } else {
            letter
        };
        if self.dialect.types && room && generic {
            self.state.push(Nest::Generic);
            self.sink.push(at..at + 1, Punctuation);
            self.state.prev = Prev::Other;
            return at + 1;
        }
        self.symbol(at)
    }

    /// The punctuation or operator at `at`, a byte that starts no comment, literal, word,
    /// whitespace or tag: its end.
    pub(super) fn symbol(&mut self, at: usize) -> usize {
        let rest = &self.line.as_bytes()[at..];
        let prev = self.state.prev;
        let (len, class, next) = match rest {
            [b'(', ..] if prev == Prev::Condition => self.open(Nest::Condition, Prev::Other),
            [b'(', ..] => self.open(Nest::Paren, Prev::KeyStart),
            [b'[', ..] => self.open(Nest::Bracket, Prev::Other),
            [b'{', ..] => {
                self.drop_generics();
                self.open(Nest::Brace, Prev::KeyStart)
            }
            [b')', ..] => {
                let condition = self.close(Nest::Paren) == Some(Nest::Condition);
                let next = if condition {
                    Prev::Other
                } else {
                    Prev::ValueEnd
                };
                (1, Some(Punctuation), next)
            }
            [b']', ..] => {
                self.close(Nest::Bracket);
                (1, Some(Punctuation), Prev::ValueEnd)
            }
            [b'}', ..] => {
                self.close(Nest::Brace);
                (1, Some(Punctuation), Prev::Other)
            }
            [b';', ..] => {
                self.drop_generics();
                (1, Some(Punctuation), Prev::KeyStart)
            }
            [b',', ..] => (1, Some(Punctuation), Prev::KeyStart),
            [b':', ..] if prev == Prev::Param => (1, Some(Punctuation), Prev::Annotation),
            [b':', ..] => (1, Some(Punctuation), Prev::Other),
            [b'?', b'.', after, ..] if !after.is_ascii_digit() => (2, Some(Punctuation), Prev::Dot),
            [b'?', b'.'] => (2, Some(Punctuation), Prev::Dot),
            [b'?', ..] if prev == Prev::Param => (1, Some(Punctuation), Prev::Param),
            [b'.', b'.', b'.', ..] => (3, Some(Operator), Prev::Other),
            [b'.', ..] => (1, Some(Punctuation), Prev::Dot),
            [b'>', ..] if self.state.top() == Some(Nest::Generic) => {
                self.state.pop();
                (1, Some(Punctuation), Prev::ValueEnd)
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
    /// popped. A `)` closes a `Condition` too.
    fn close(&mut self, closer: Nest) -> Option<Nest> {
        self.drop_generics();
        let top = self.state.top();
        let pops = match top {
            Some(Nest::Condition) => closer == Nest::Paren,
            Some(top) => top == closer,
            None => true,
        };
        if pops {
            self.state.pop();
        }
        top.filter(|_| pops)
    }

    fn drop_generics(&mut self) {
        while self.state.top() == Some(Nest::Generic) {
            self.state.pop();
        }
    }
}
