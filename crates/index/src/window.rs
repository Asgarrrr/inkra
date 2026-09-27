//! The part of a line shown around a match: a search hit's context, a quick-open excerpt.

use std::ops::Range;

/// Bytes of the hit's line kept before and after it.
const BEFORE: usize = 60;
const AFTER: usize = 100;

/// `hit` widened within its line by up to `BEFORE`/`AFTER` bytes, on char boundaries.
pub(crate) fn around(text: &str, hit: Range<usize>) -> Range<usize> {
    // Only the window is scanned for the line's ends: a line can be megabytes long.
    let from = text.ceil_char_boundary(hit.start.saturating_sub(BEFORE));
    let to = text.floor_char_boundary((hit.end + AFTER).min(text.len()));
    let from = text[from..hit.start]
        .rfind('\n')
        .map_or(from, |at| from + at + 1);
    let to = text[hit.end..to].find('\n').map_or(to, |at| hit.end + at);
    from..to
}

#[cfg(test)]
mod tests;
