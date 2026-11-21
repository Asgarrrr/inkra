//! HTML: tags, attributes, entities, comments across lines and the doctype; `<script>` content
//! lexed by JavaScript and `<style>` content by CSS, across lines (code highlighting spec, D4,
//! Languages).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{self, Attribute, Punctuation, String, Tag};
use crate::scan::{Sink, block_comment, drive, end_while, entity_end, quoted, skip_spaces};
use crate::state::{LineState, Nest, Open, Prev};
use crate::{css, javascript};

/// The embedded language open at the line state's bottom, if any; an empty stack holds only
/// `Nest::default()`.
fn embedded(state: &LineState) -> Option<Nest> {
    Some(state.stack[0]).filter(|bottom| matches!(bottom, Nest::Script | Nest::Style))
}

fn is_tag_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-:_.".contains(&byte)
}

/// An attribute name's byte: anything but whitespace, a quote, `=`, `/`, `<` or `>`.
fn is_attribute_name(byte: u8) -> bool {
    !(byte.is_ascii_whitespace() || b"\"'=/<>".contains(&byte))
}

/// An unquoted attribute value's byte: anything but whitespace or `>`.
fn is_bare_value(byte: u8) -> bool {
    !(byte.is_ascii_whitespace() || byte == b'>')
}

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
}

impl Tokens<'_> {
    /// Lexes what starts at `at`, with nothing of HTML's own open there: its end.
    fn step(&mut self, at: usize) -> usize {
        match (embedded(&self.state), self.state.prev) {
            (Some(language), _) => self.embedded(at, language),
            (None, Prev::HtmlTag | Prev::HtmlScript | Prev::HtmlStyle) => self.tag(at),
            (None, _) => self.text(at),
        }
    }

    /// Hands the line from `at` up to its closing tag to the embedded language, then lexes the
    /// closing tag: its end.
    fn embedded(&mut self, at: usize, language: Nest) -> usize {
        let bytes = self.line.as_bytes();
        let (closer, lex): (&[u8], crate::Lexer) = if language == Nest::Script {
            (b"</script", javascript::lex)
        } else {
            (b"</style", css::lex)
        };
        let cut = bytes[at..]
            .windows(closer.len())
            .position(|window| window.eq_ignore_ascii_case(closer))
            .map_or(bytes.len(), |len| at + len);
        self.state = lex(
            self.state,
            &self.line[at..cut],
            self.sink.start + at,
            self.sink.out,
        );
        if cut == bytes.len() {
            return cut;
        }
        self.state = LineState::default();
        self.text(cut)
    }

    /// What starts at `at` between tags: a tag, a comment, the doctype, an entity, or text.
    fn text(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let rest = &bytes[at..];
        let letter = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_alphabetic);
        if rest.starts_with(b"<!--") {
            self.comment(at, at + 4)
        } else if rest.starts_with(b"<!") {
            let end = end_while(bytes, at + 2, |byte| byte != b'>');
            self.sink.push(at..at + 2, Punctuation);
            self.sink.push(at + 2..end, Tag);
            let close = end + usize::from(end < bytes.len());
            self.sink.token(end..close, Punctuation)
        } else if rest.starts_with(b"</") && letter(at + 2) {
            self.sink.push(at..at + 2, Punctuation);
            let end = end_while(bytes, at + 2, is_tag_name);
            self.sink.push(at + 2..end, Tag);
            self.state.prev = Prev::HtmlTag;
            end
        } else if rest[0] == b'<' && letter(at + 1) {
            self.sink.push(at..at + 1, Punctuation);
            let end = end_while(bytes, at + 1, is_tag_name);
            let name = &self.line[at + 1..end];
            self.sink.push(at + 1..end, Tag);
            self.state.prev = if name.eq_ignore_ascii_case("script") {
                Prev::HtmlScript
            } else if name.eq_ignore_ascii_case("style") {
                Prev::HtmlStyle
            } else {
                Prev::HtmlTag
            };
            end
        } else if let Some(end) = (rest[0] == b'&').then(|| entity_end(bytes, at)).flatten() {
            self.sink.push(at..end, String);
            end
        } else {
            bytes[at + 1..]
                .iter()
                .position(|byte| b"<&".contains(byte))
                .map_or(bytes.len(), |len| at + 1 + len)
        }
    }

    /// Scans a comment from `at` up to its `-->`, as one token from `token`: its end on the line.
    fn comment(&mut self, token: usize, at: usize) -> usize {
        let end;
        (end, self.state.open) =
            block_comment(&mut self.sink, self.line.as_bytes(), token, at, b"-->");
        end
    }

    /// What starts at `at` inside a tag: an attribute, its value, or the tag's end.
    fn tag(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'>') => {
                self.state.prev = Prev::Other;
                self.sink.token(at..at + 2, Punctuation)
            }
            b'>' => {
                match self.state.prev {
                    Prev::HtmlScript => self.state.push(Nest::Script),
                    Prev::HtmlStyle => self.state.push(Nest::Style),
                    _ => {}
                }
                self.state.prev = Prev::Other;
                self.sink.token(at..at + 1, Punctuation)
            }
            b'=' => {
                self.sink.push(at..at + 1, Punctuation);
                let value = skip_spaces(bytes, at + 1);
                match bytes.get(value) {
                    Some(&quote @ (b'"' | b'\'')) => self.value(value, value + 1, quote),
                    Some(b'>') | None => value,
                    Some(_) => {
                        let end = end_while(bytes, value, is_bare_value);
                        self.sink.push(value..end, String);
                        end
                    }
                }
            }
            quote @ (b'"' | b'\'') => self.value(at, at + 1, quote),
            byte if is_attribute_name(byte) => {
                let end = end_while(bytes, at, is_attribute_name);
                self.sink.token(at..end, Attribute)
            }
            _ => at + 1,
        }
    }

    /// A quoted attribute value from `at`, as one token from `token`: its end on the line, where
    /// it stays open unclosed.
    fn value(&mut self, token: usize, at: usize, quote: u8) -> usize {
        let bytes = self.line.as_bytes();
        let close = quoted(bytes, at, quote, false, false);
        self.state.open = match close {
            Some(_) => Open::Nothing,
            None => Open::Quoted {
                quote,
                triple: false,
                escapes: false,
            },
        };
        let end = close.unwrap_or(bytes.len());
        self.sink.push(token..end, String);
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
        state,
    };
    let at = match (embedded(&state), state.open) {
        (None, Open::BlockComment { .. }) => tokens.comment(0, 0),
        (None, Open::Quoted { quote, .. }) => tokens.value(0, 0, quote),
        _ => 0,
    };
    drive(line.len(), at, |at| {
        let open = embedded(&tokens.state).is_none() && tokens.state.open != Open::Nothing;
        (!open).then(|| tokens.step(at))
    });
    tokens.state
}

#[cfg(test)]
mod tests;
