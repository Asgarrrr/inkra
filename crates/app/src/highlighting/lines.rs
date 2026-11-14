//! A code block's content lines, from the markdown parse (code highlighting spec, D3).

use inkra_markdown::{CodeBlock, Span, Styles};
use inkra_text::ByteOffset;

/// Each physical line of `block`, in order: its code span, or an empty line at its end when it
/// has none, as a fence line has.
pub(super) fn content_lines<'a>(
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
                if span.styles.contains(Styles::CODE_BLOCK) && !span.styles.contains(Styles::MARKER)
                {
                    content = (
                        span.range.start,
                        &text[span.range.start.get()..span.range.end.get()],
                    );
                }
            }
            content
        })
}
