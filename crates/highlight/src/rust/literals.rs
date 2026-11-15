//! Rust comments, strings, char literals, lifetimes and numbers, scanned byte by byte: every
//! byte they stop on is ASCII, so every end is a char boundary.

use crate::TokenClass;
use crate::state::Open;

use super::{is_ident_start, word_end};

/// Scans a block comment open at `depth` from `at`: the end of its closer, or the line end with
/// what stays open.
pub(super) fn block_comment(bytes: &[u8], mut at: usize, mut depth: u8) -> (usize, Open) {
    while let Some(&byte) = bytes.get(at) {
        match (byte, bytes.get(at + 1)) {
            (b'/', Some(b'*')) => {
                depth = depth.saturating_add(1);
                at += 2;
            }
            (b'*', Some(b'/')) => {
                depth -= 1;
                at += 2;
                if depth == 0 {
                    return (at, Open::Nothing);
                }
            }
            _ => at += 1,
        }
    }
    (bytes.len(), Open::BlockComment { depth })
}

/// Scans a string open from `at`, escaped when `raw_hashes` is `None`: the end of its closer, or
/// the line end with the string still open.
pub(super) fn string(bytes: &[u8], mut at: usize, raw_hashes: Option<u8>) -> (usize, Open) {
    while let Some(&byte) = bytes.get(at) {
        match (byte, raw_hashes) {
            (b'\\', None) => at += 2,
            (b'"', None) => return (at + 1, Open::Nothing),
            (b'"', Some(hashes)) => {
                let end = at + 1 + usize::from(hashes);
                if bytes
                    .get(at + 1..end)
                    .is_some_and(|tail| tail.iter().all(|&byte| byte == b'#'))
                {
                    return (end, Open::Nothing);
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
    (bytes.len(), Open::Str { raw_hashes })
}

/// After a raw prefix ending at `at`: the body's start and the hash count, saturating, when `#`s
/// and a `"` follow.
pub(super) fn raw_open(bytes: &[u8], at: usize) -> Option<(usize, u8)> {
    let hashes = bytes[at..].iter().take_while(|&&byte| byte == b'#').count();
    (bytes.get(at + hashes) == Some(&b'"'))
        .then(|| (at + hashes + 1, u8::try_from(hashes).unwrap_or(u8::MAX)))
}

/// The end of the number at the digit at `at`: `[0-9A-Za-z_]`, at most one `.` before any
/// exponent, and a decimal exponent's sign.
pub(super) fn number(bytes: &[u8], at: usize) -> usize {
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

/// The char literal or lifetime at the `'` at `at`: its end and class. Neither spans lines, so
/// an unclosed char literal runs to the line end.
pub(super) fn quote(line: &str, at: usize) -> (usize, TokenClass) {
    let bytes = line.as_bytes();
    let next = at + 1;
    let Some(char) = line[next..].chars().next() else {
        return (line.len(), TokenClass::String);
    };
    let end = if char == '\\' {
        escape_end(line, next)
    } else {
        next + char.len_utf8()
    };
    if bytes.get(end) == Some(&b'\'') {
        (end + 1, TokenClass::String)
    } else if is_ident_start(bytes[next]) {
        let name = if bytes[next..].starts_with(b"r#")
            && bytes.get(next + 2).copied().is_some_and(is_ident_start)
        {
            next + 2
        } else {
            next
        };
        (word_end(bytes, name), TokenClass::Type)
    } else {
        (line.len(), TokenClass::String)
    }
}

/// The end of the escape at the `\` at `at`: `\u{…}` to its `}` on the line, else one char.
fn escape_end(line: &str, at: usize) -> usize {
    let rest = &line[at + 1..];
    if rest.starts_with("u{")
        && let Some(close) = rest.find('}')
    {
        at + 1 + close + 1
    } else {
        at + 1 + rest.chars().next().map_or(0, char::len_utf8)
    }
}

#[cfg(test)]
mod tests;
