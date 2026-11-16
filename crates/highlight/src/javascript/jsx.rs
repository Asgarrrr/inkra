//! JSX: tags, their attributes, and the children between them, across lines (code highlighting
//! spec, D4).

use crate::TokenClass::{Attribute, Punctuation, String, Tag};
use crate::scan::{end_while, entity_end, is_ident_start, quoted, skip_spaces};
use crate::state::{Nest, Prev};

use super::{Tokens, is_ident};

/// A byte of a tag or attribute name: an identifier byte, `-` or `:`.
fn is_name(byte: u8) -> bool {
    is_ident(byte) || byte == b'-' || byte == b':'
}

impl Tokens<'_> {
    /// The `<` at `at` and the tag name after it, empty for a fragment's `<>`: its end.
    pub(super) fn jsx_open(&mut self, at: usize) -> usize {
        self.state.push(Nest::JsxTag);
        self.sink.push(at..at + 1, Punctuation);
        self.tag_name(at + 1)
    }

    /// A tag name from `at`: each `.`-separated part a `Tag`, as Zed colours a member expression.
    fn tag_name(&mut self, at: usize) -> usize {
        let end = end_while(self.line.as_bytes(), at, |byte| {
            is_name(byte) || byte == b'.'
        });
        let mut part = at;
        for (len, _) in self.line[at..end].match_indices('.') {
            self.sink.push(part..at + len, Tag);
            self.sink.push(at + len..at + len + 1, Punctuation);
            part = at + len + 1;
        }
        self.sink.token(part..end, Tag)
    }

    /// What starts at `at` inside a tag: attributes, their values, an embedded `{…}`, the end.
    pub(super) fn jsx_tag(&mut self, at: usize, closing: bool) -> usize {
        let bytes = self.line.as_bytes();
        match bytes[at] {
            b'{' => {
                self.sink.push(at..at + 1, Punctuation);
                self.state.push(Nest::Brace);
                self.state.prev = Prev::Other;
                at + 1
            }
            b'/' => {
                let close = skip_spaces(bytes, at + 1);
                if bytes.get(close) != Some(&b'>') {
                    return at + 1;
                }
                self.sink.push(at..close + 1, Punctuation);
                self.state.pop();
                self.state.prev = Prev::ValueEnd;
                close + 1
            }
            b'>' => {
                self.sink.push(at..at + 1, Punctuation);
                self.state.pop();
                if closing {
                    self.state.prev = Prev::ValueEnd;
                } else {
                    self.state.push(Nest::JsxText);
                }
                at + 1
            }
            b'=' => {
                self.sink.push(at..at + 1, Punctuation);
                at + 1
            }
            quote @ (b'"' | b'\'') => {
                let end = quoted(bytes, at + 1, quote, false, false).unwrap_or(bytes.len());
                self.sink.push(at..end, String);
                end
            }
            byte if is_ident_start(byte) => {
                let end = end_while(bytes, at, is_name);
                self.sink.push(at..end, Attribute);
                end
            }
            _ => at + 1,
        }
    }

    /// What starts at `at` among an element's children: text, an entity, an embedded `{…}`, a
    /// child element or the closing tag.
    pub(super) fn jsx_text(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        match bytes[at] {
            b'{' => {
                self.sink.push(at..at + 1, Punctuation);
                self.state.push(Nest::Brace);
                self.state.prev = Prev::Other;
                at + 1
            }
            b'<' if next == Some(b'/') => {
                self.state.pop();
                self.state.push(Nest::JsxClosingTag);
                self.sink.push(at..at + 2, Punctuation);
                self.tag_name(at + 2)
            }
            b'<' if self.state.room()
                && (next.is_some_and(is_ident_start) || next == Some(b'>')) =>
            {
                self.jsx_open(at)
            }
            b'&' => self.entity(at),
            _ => bytes[at + 1..]
                .iter()
                .position(|byte| b"{<&".contains(byte))
                .map_or(bytes.len(), |len| at + 1 + len),
        }
    }

    /// The character reference at the `&` at `at`, a `String`, or the `&` alone: its end.
    fn entity(&mut self, at: usize) -> usize {
        let Some(end) = entity_end(self.line.as_bytes(), at) else {
            return at + 1;
        };
        self.sink.push(at..end, String);
        end
    }
}
