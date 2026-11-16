//! Byte scanning every lexer shares: words, spaces, numbers, quoted strings, and the line's token
//! sink. Every byte a scan stops on is ASCII, so every end is a char boundary.

use std::cell::OnceCell;
use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;
use crate::state::Open;

/// A word byte is ASCII alphanumeric, `_` or non-ASCII, so a word ends on a char boundary.
pub(crate) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

pub(crate) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii()
}

/// The end of the run from `at` of bytes that `keep` accepts.
pub(crate) fn end_while(bytes: &[u8], at: usize, keep: impl Fn(u8) -> bool) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| !keep(byte))
        .map_or(bytes.len(), |len| at + len)
}

pub(crate) fn word_end(bytes: &[u8], at: usize) -> usize {
    end_while(bytes, at, is_word)
}

pub(crate) fn skip_spaces(bytes: &[u8], at: usize) -> usize {
    end_while(bytes, at, |byte| byte.is_ascii_whitespace())
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

/// The end of `word(.word)*` from the word at `at`.
pub(crate) fn dotted_end(bytes: &[u8], at: usize) -> usize {
    let mut end = word_end(bytes, at);
    while bytes.get(end) == Some(&b'.') && bytes.get(end + 1).copied().is_some_and(is_ident_start) {
        end = word_end(bytes, end + 1);
    }
    end
}

/// Longer than a generic list written by hand, so most lines never pair the whole line.
const LOOK_AHEAD: usize = 64;

/// Where a line's generic lists close. A list that a short look-ahead leaves open is found in a
/// pairing of the whole line, made once, so the line lexes in linear time.
pub(crate) struct Generics<'a> {
    bytes: &'a [u8],
    ends: OnceCell<Vec<(usize, usize)>>,
}

impl<'a> Generics<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            ends: OnceCell::new(),
        }
    }

    /// The end of the generic list whose `<` is at `open`, if it closes on the line before a `;`,
    /// `{` or `}`; the `>` of `->` and `=>` closes none.
    pub(crate) fn end(&self, open: usize) -> Option<usize> {
        if self.bytes.get(open) != Some(&b'<') {
            return None;
        }
        self.look_ahead(open)
            .unwrap_or_else(|Undecided| self.paired_end(open))
    }

    /// `end` from the whole line's pairing; kept out of line, since only long lists reach it.
    #[cold]
    #[inline(never)]
    fn paired_end(&self, open: usize) -> Option<usize> {
        let ends = self.ends.get_or_init(|| pair(self.bytes));
        let index = ends.binary_search_by_key(&open, |&(open, _)| open).ok()?;
        Some(ends[index].1)
    }

    /// `end` from the `LOOK_AHEAD` bytes from `open`, when they decide it.
    fn look_ahead(&self, open: usize) -> Result<Option<usize>, Undecided> {
        let mut depth = 0_usize;
        let limit = open + LOOK_AHEAD;
        for at in open..self.bytes.len().min(limit) {
            match angle(self.bytes, at) {
                Angle::Open => depth += 1,
                Angle::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Some(at + 1));
                    }
                }
                Angle::Stop => return Ok(None),
                Angle::Other => {}
            }
        }
        if self.bytes.len() <= limit {
            Ok(None)
        } else {
            Err(Undecided)
        }
    }
}

/// The look-ahead reached its limit with the list still open.
struct Undecided;

/// What the byte at `at` does to the generic lists open before it.
enum Angle {
    Open,
    Close,
    /// Closes none and ends them all: a `;`, `{` or `}`.
    Stop,
    Other,
}

fn angle(bytes: &[u8], at: usize) -> Angle {
    match bytes[at] {
        b'<' => Angle::Open,
        b'>' if !matches!(
            at.checked_sub(1).map(|before| bytes[before]),
            Some(b'-' | b'=')
        ) =>
        {
            Angle::Close
        }
        b';' | b'{' | b'}' => Angle::Stop,
        _ => Angle::Other,
    }
}

/// Every `<` whose list closes, with its end, as `(open, end)` in the line's order.
fn pair(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut ends = Vec::new();
    // Indices into `ends` of the lists still open.
    let mut opens = Vec::new();
    for at in 0..bytes.len() {
        match angle(bytes, at) {
            Angle::Open => {
                opens.push(ends.len());
                ends.push((at, None));
            }
            Angle::Close => {
                if let Some(index) = opens.pop() {
                    ends[index].1 = Some(at + 1);
                }
            }
            Angle::Stop => opens.clear(),
            Angle::Other => {}
        }
    }
    // Open ones dropped, so a line of lists that never close searches an empty table.
    ends.into_iter()
        .filter_map(|(open, end)| Some((open, end?)))
        .collect()
}

/// Scans a comment that ends at `closer` and does not nest, from `at` as one `Comment` from
/// `token`: its end on the line, and what stays open there.
pub(crate) fn block_comment(
    sink: &mut Sink,
    bytes: &[u8],
    token: usize,
    at: usize,
    closer: &[u8],
) -> (usize, Open) {
    let close = bytes[at..]
        .windows(closer.len())
        .position(|window| window == closer)
        .map(|len| at + len + closer.len());
    sink.push(token..close.unwrap_or(bytes.len()), TokenClass::Comment);
    match close {
        Some(end) => (end, Open::Nothing),
        None => (bytes.len(), Open::BlockComment { depth: 1 }),
    }
}

/// The end of the character reference at the `&` at `at`, `&name;` or `&#…;`, if one is there.
pub(crate) fn entity_end(bytes: &[u8], at: usize) -> Option<usize> {
    let body = at + 1 + usize::from(bytes.get(at + 1) == Some(&b'#'));
    let len = bytes[body..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric())
        .count();
    (len > 0 && bytes.get(body + len) == Some(&b';')).then_some(body + len + 1)
}

/// Runs `step` from `at` to the line end, or until it returns `None` because something stays
/// open. A step that does not advance panics: a stuck lexer would hang its background task.
pub(crate) fn drive(len: usize, mut at: usize, mut step: impl FnMut(usize) -> Option<usize>) {
    while at < len {
        let Some(next) = step(at) else { return };
        assert!(next > at, "a lexer step went from {at} to {next}");
        at = next;
    }
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

    /// Pushes `range` as `push` does: its end.
    pub(crate) fn token(&mut self, range: Range<usize>, class: TokenClass) -> usize {
        let end = range.end;
        self.push(range, class);
        end
    }
}

#[cfg(test)]
mod tests;
