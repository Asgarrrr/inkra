use std::ops::Range;

use inkra_markdown::markup;

/// A line's bytes parsed around a piece for its markup: markup closing past them stays.
pub(super) const LINE_BYTES: usize = 1 << 10;

/// A piece of a line without its markup, and where the markup was.
pub(super) struct Plain {
    pub(super) text: String,
    /// Byte ranges of the marked piece, ascending and disjoint.
    dropped: Vec<Range<usize>>,
}

impl Plain {
    /// Parses all of `context`, so a delimiter whose partner lies outside `piece` still drops.
    pub(super) fn new(context: &str, at_line_start: bool, piece: Range<usize>) -> Self {
        // An indent would read as a code block, and `piece` never shows it as markup.
        let indent = if at_line_start {
            context.len() - context.trim_start().len()
        } else {
            0
        };
        let dropped: Vec<Range<usize>> = markup(&context[indent..], at_line_start)
            .into_iter()
            .map(|range| range.start.0 + indent..range.end.0 + indent)
            .map(|range| range.start.max(piece.start)..range.end.min(piece.end))
            .filter(|range| range.start < range.end)
            .map(|range| range.start - piece.start..range.end - piece.start)
            .collect();
        let piece = &context[piece];
        let mut text = String::with_capacity(piece.len());
        let mut at = 0;
        for range in &dropped {
            text.push_str(&piece[at..range.start]);
            at = range.end;
        }
        text.push_str(&piece[at..]);
        Self { text, dropped }
    }

    /// Where `offset` of the marked piece lands in `text`; inside markup, where the markup was.
    pub(super) fn map(&self, offset: usize) -> usize {
        let dropped: usize = self
            .dropped
            .iter()
            .map(|range| range.end.min(offset).saturating_sub(range.start))
            .sum();
        offset - dropped
    }
}
