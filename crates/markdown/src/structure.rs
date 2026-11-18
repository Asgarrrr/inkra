//! What `⌘L` reads of the parse: where items and quotes open, and the lines it skips (task boxes
//! spec, D1, D6).

use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::gutters::marker_start;
use crate::parse::without_line_ending;
use crate::spans::{line_start, to_offsets};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Structure {
    /// Each list item's marker, the bullet or the first digit, in document order.
    pub items: Vec<ByteOffset>,
    /// Each block quote's start, where pulldown-cmark starts its range, in document order.
    pub quotes: Vec<ByteOffset>,
    /// Each code block, front matter, table, HTML block and ATX heading, from its first line's
    /// start to its last line's end, and each line holding a task with no box. Sorted; none holds
    /// another.
    pub skipped: Vec<Range<ByteOffset>>,
}

/// Fills a `Structure` from the parse's events, one call per element it opens.
#[derive(Default)]
pub(crate) struct Collector {
    structure: Structure,
}

impl Collector {
    pub(crate) fn start(&mut self, text: &str, tag: &Tag<'_>, range: &Range<usize>) {
        match tag {
            Tag::Item => {
                let marker = marker_start(text, range.start);
                self.structure.items.push(ByteOffset::new(marker));
            }
            Tag::BlockQuote(_) => {
                self.structure.quotes.push(ByteOffset::new(range.start));
            }
            Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_) | Tag::HtmlBlock => {
                self.skip(text, range);
            }
            // A setext heading's range holds its underline's line too.
            Tag::Heading { .. } if !text[without_line_ending(text, range)].contains('\n') => {
                self.skip(text, range);
            }
            _ => {}
        }
    }

    /// The structure, with `unboxed`, the lines holding a task with no box, merged into `skipped`.
    pub(crate) fn finish(self, unboxed: Vec<Range<usize>>) -> Structure {
        let mut structure = self.structure;
        structure
            .skipped
            .extend(unboxed.into_iter().map(to_offsets));
        // Two sorted runs: the stable sort merges them in linear time.
        structure.skipped.sort_by_key(|range| range.start);
        debug_assert!(
            structure.skipped.is_sorted_by(|a, b| a.end < b.start),
            "a skipped range holds another"
        );
        structure
    }

    fn skip(&mut self, text: &str, range: &Range<usize>) {
        let start = line_start(text, range.start);
        let end = without_line_ending(text, range).end;
        self.structure.skipped.push(to_offsets(start..end));
    }
}

/// The marker of the innermost item one pass reads in `text`, a single line, as `parse` reads it:
/// `text` ends with a newline.
pub(crate) fn innermost_item(text: &str) -> Option<usize> {
    let mut collector = Collector::default();
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        if let Event::Start(tag) = &event {
            collector.start(text, tag, &range);
        }
    }
    collector.structure.items.last().map(|marker| marker.get())
}

#[cfg(test)]
mod tests;
