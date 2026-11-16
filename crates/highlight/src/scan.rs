//! Byte scanning every lexer shares: words, spaces, numbers, quoted strings, and the line's token
//! sink. Every byte a scan stops on is ASCII, so every end is a char boundary.

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;

/// A word byte is ASCII alphanumeric, `_` or non-ASCII, so a word ends on a char boundary.
pub(crate) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

pub(crate) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii()
}

pub(crate) fn word_end(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| !is_word(byte))
        .map_or(bytes.len(), |len| at + len)
}

pub(crate) fn skip_spaces(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map_or(bytes.len(), |len| at + len)
}

/// Two or more of `[A-Z0-9_]` from an ASCII uppercase letter.
pub(crate) fn is_all_caps(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_uppercase()
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The end of the number at `at`, a digit or a `.` before one: `[0-9A-Za-z_]`, at most one `.`
/// before any exponent, and a decimal exponent's sign.
pub(crate) fn number(bytes: &[u8], at: usize) -> usize {
    let decimal = !(bytes[at] == b'0' && matches!(bytes.get(at + 1), Some(b'x' | b'o' | b'b')));
    let mut end = at;
    loop {
        end += bytes[end..]
            .iter()
            .take_while(|&&byte| byte.is_ascii_alphanumeric() || byte == b'_')
            .count();
        let taken = &bytes[at..end];
        let exponent = decimal && taken.iter().any(|&byte| matches!(byte, b'e' | b'E'));
        let joins = bytes
            .get(end + 1)
            .is_none_or(|&byte| byte != b'.' && !is_ident_start(byte));
        match bytes.get(end) {
            Some(b'+' | b'-') if decimal && matches!(taken.last(), Some(b'e' | b'E')) => end += 1,
            Some(b'.') if !exponent && !taken.contains(&b'.') && joins => end += 1,
            _ => return end,
        }
    }
}

/// Scans a string open from `at` that closes on `quote`, thrice when `triple`; a `\` skips the
/// next byte when `escapes`. The end of its closer, or `None` at the line end.
pub(crate) fn quoted(
    bytes: &[u8],
    mut at: usize,
    quote: u8,
    triple: bool,
    escapes: bool,
) -> Option<usize> {
    let closer = if triple { 3 } else { 1 };
    while let Some(&byte) = bytes.get(at) {
        if byte == b'\\' && escapes {
            at += 2;
        } else if byte == quote
            && bytes
                .get(at..at + closer)
                .is_some_and(|run| run.iter().all(|&byte| byte == quote))
        {
            return Some(at + closer);
        } else {
            at += 1;
        }
    }
    None
}

/// One line's tokens, pushed at the line's start offset.
pub(crate) struct Sink<'a> {
    pub(crate) start: ByteOffset,
    pub(crate) out: &'a mut Vec<(Range<ByteOffset>, TokenClass)>,
}

impl Sink<'_> {
    /// Pushes `range`, relative to the line, unless it is empty.
    pub(crate) fn push(&mut self, range: Range<usize>, class: TokenClass) {
        if !range.is_empty() {
            self.out
                .push((self.start + range.start..self.start + range.end, class));
        }
    }
}
