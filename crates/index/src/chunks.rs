use std::ops::Range;

/// At most `n` non-empty ranges, in order, covering `0..len`; none when `len` is 0.
pub(crate) fn split(len: usize, n: usize) -> Vec<Range<usize>> {
    let n = n.clamp(1, len.max(1));
    (0..n)
        .map(|k| k * len / n..(k + 1) * len / n)
        .filter(|range| !range.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
