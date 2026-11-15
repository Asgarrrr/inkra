//! Which block chunks the open function lands (code highlighting spec, D8).

use std::iter;

/// The longest text, in bytes, the open function parses; a longer one lands no chunk (D8).
pub const OPEN_PARSE_LIMIT: usize = 500_000;

/// The chunks a text of `len` bytes lands at open: none over `OPEN_PARSE_LIMIT`, where `chunks`
/// is never called; else each chunk `chunks` yields, in document order, while `before_deadline`
/// holds before it is made.
#[must_use]
pub fn open_chunks<I: IntoIterator>(
    len: usize,
    chunks: impl FnOnce() -> I,
    mut before_deadline: impl FnMut() -> bool,
) -> Vec<I::Item> {
    if len > OPEN_PARSE_LIMIT {
        return Vec::new();
    }
    let mut chunks = chunks().into_iter();
    iter::from_fn(|| before_deadline().then(|| chunks.next()).flatten()).collect()
}

#[cfg(test)]
mod tests;
