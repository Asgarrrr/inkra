//! JavaScript's template strings, whose text spans lines and whose `${…}` nests, and regex
//! literals with their classes and flags (code highlighting spec, D4).

use crate::TokenClass::{Keyword, Punctuation, String};
use crate::scan::end_while;
use crate::state::{Nest, Prev};

use super::{Tokens, is_ident};

impl Tokens<'_> {
    /// A template's text from `at`, as one `String` from `token`, up to its closing backtick, a
    /// `${` or the line end: its end on the line.
    pub(super) fn template(&mut self, token: usize, mut at: usize) -> usize {
        let bytes = self.line.as_bytes();
        while let Some(&byte) = bytes.get(at) {
            match byte {
                b'\\' => at += 2,
                b'`' => {
                    self.sink.push(token..at + 1, String);
                    self.state.pop();
                    self.state.prev = Prev::ValueEnd;
                    return at + 1;
                }
                b'$' if bytes.get(at + 1) == Some(&b'{') => {
                    self.sink.push(token..at, String);
                    self.sink.push(at..at + 2, Punctuation);
                    self.state.push(Nest::Brace);
                    self.state.prev = Prev::Other;
                    return at + 2;
                }
                _ => at += 1,
            }
        }
        self.sink.push(token..bytes.len(), String);
        bytes.len()
    }

    /// The regex literal whose `/` is at `at`, a `String`, and its flags, a `Keyword` as in Zed; a
    /// regex cannot span lines, so an unclosed one ends at the line end.
    pub(super) fn regex(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let mut end = at + 1;
        let mut class = false;
        let close = loop {
            match bytes.get(end) {
                None => break None,
                Some(b'\\') => end += 2,
                Some(b'[') => {
                    class = true;
                    end += 1;
                }
                Some(b']') => {
                    class = false;
                    end += 1;
                }
                Some(b'/') if !class => break Some(end + 1),
                Some(_) => end += 1,
            }
        };
        self.state.prev = Prev::ValueEnd;
        let Some(close) = close else {
            self.sink.push(at..bytes.len(), String);
            return bytes.len();
        };
        self.sink.push(at..close, String);
        let flags = end_while(bytes, close, is_ident);
        self.sink.push(close..flags, Keyword);
        flags
    }
}
