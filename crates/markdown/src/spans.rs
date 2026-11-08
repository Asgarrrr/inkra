//! Byte-range primitives the event walkers share: code span bounds, line pieces, merged spans.

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::{Span, Styles};

/// A `Code` event's range includes its backticks; returns where the content starts and ends.
pub(crate) fn backtick_runs(text: &str, range: &Range<usize>) -> (usize, usize) {
    let code = &text.as_bytes()[range.clone()];
    let open = code.iter().take_while(|&&byte| byte == b'`').count();
    let close = code[open..]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'`')
        .count();
    (range.start + open, range.end - close)
}

/// Appends a span, merged into the last one when they touch and share their styles.
pub(crate) fn push(spans: &mut Vec<Span>, range: Range<usize>, styles: Styles) {
    if range.is_empty() || styles.is_empty() {
        return;
    }
    let range = to_offsets(range);
    if let Some(last) = spans.last_mut()
        && last.range.end == range.start
        && last.styles == styles
    {
        last.range.end = range.end;
        return;
    }
    spans.push(Span { range, styles });
}

/// The pieces of `range` between its `\n` bytes, which they exclude.
pub(crate) fn lines(text: &str, range: Range<usize>) -> impl Iterator<Item = Range<usize>> {
    let mut start = range.start;
    text.as_bytes()[range]
        .split(|&byte| byte == b'\n')
        .map(move |line| {
            let piece = start..start + line.len();
            start = piece.end + 1;
            piece
        })
}

pub(crate) fn to_offsets(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}
