//! A code block's content lines, from the parse (code highlighting spec, D3).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use inkra_text::ByteOffset;

use crate::{CodeBlock, Span, Styles};

/// Each physical line of `block`, in order: its code span, or an empty line at its end when it
/// has none, as a fence line has.
pub fn content_lines<'a>(
    text: &'a str,
    spans: &'a [Span],
    block: &CodeBlock,
) -> impl Iterator<Item = (ByteOffset, &'a str)> + use<'a> {
    let mut next = spans.partition_point(|span| span.range.end <= block.range.start);
    let mut line_start = block.range.start;
    text[block.range.start.get()..block.range.end.get()]
        .split('\n')
        .map(move |line| {
            let line_end = line_start + line.len();
            line_start = line_end + 1;
            let mut content = (line_end, "");
            while let Some(span) = spans.get(next)
                && span.range.start < line_end
            {
                next += 1;
                if is_content(span) {
                    content = (
                        span.range.start,
                        &text[span.range.start.get()..span.range.end.get()],
                    );
                }
            }
            content
        })
}

/// `CodeBlock::shape` of the block over `range`: the content spans `content_lines` reads and the
/// language word, relative to the block's start, hashed.
pub(crate) fn shape(
    spans: &[Span],
    range: &Range<ByteOffset>,
    language: Option<&Range<ByteOffset>>,
) -> u64 {
    let relative = |at: ByteOffset| at - range.start;
    let mut hasher = DefaultHasher::new();
    language
        .map(|word| relative(word.start)..relative(word.end))
        .hash(&mut hasher);
    let first = spans.partition_point(|span| span.range.end <= range.start);
    for span in spans[first..]
        .iter()
        .take_while(|span| span.range.start < range.end)
        .filter(|span| is_content(span))
    {
        (relative(span.range.start)..relative(span.range.end)).hash(&mut hasher);
    }
    hasher.finish()
}

fn is_content(span: &Span) -> bool {
    span.styles.contains(Styles::CODE_BLOCK) && !span.styles.contains(Styles::MARKER)
}

#[cfg(test)]
mod tests;
