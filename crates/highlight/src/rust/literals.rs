//! Rust comments, strings, char literals and lifetimes, scanned byte by byte: every byte they
//! stop on is ASCII, so every end is a char boundary.

use crate::TokenClass;
use crate::scan::{is_ident_start, quoted, word_end};
use crate::state::Open;

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
    let Some(hashes) = raw_hashes else {
        return quoted(bytes, at, b'"', false, true)
            .map_or((bytes.len(), Open::Str { raw_hashes }), |end| {
                (end, Open::Nothing)
            });
    };
    while let Some(&byte) = bytes.get(at) {
        let end = at + 1 + usize::from(hashes);
        if byte == b'"'
            && bytes
                .get(at + 1..end)
                .is_some_and(|tail| tail.iter().all(|&byte| byte == b'#'))
        {
            return (end, Open::Nothing);
        }
        at += 1;
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
