//! What `⌘L` reads of the parse: where items and quotes open, and the lines it skips (task boxes
//! spec, D1, D6).

use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::items::marker_start;
use crate::parse::without_line_ending;
use crate::spans::{line_start, to_offsets};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Structure {
    /// Each list item, in document order.
    pub items: Vec<Item>,
    /// Each block quote's start, where pulldown-cmark starts its range, in document order.
    pub quotes: Vec<ByteOffset>,
    /// Each code block, front matter, table, HTML block and ATX heading, from its first line's
    /// start to its last line's end, and each line holding a task with no box. Sorted; none holds
    /// another.
    pub skipped: Vec<Range<ByteOffset>>,
}

/// A list item as the list commands read it (list editing spec, D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// Its marker, the bullet or the first digit.
    pub marker: ByteOffset,
    /// Its range's end without its last line ending: its block holds the lines that start before
    /// it.
    pub end: ByteOffset,
    /// Its parent item's index in `Structure::items`; `None` for a top-level item.
    pub parent: Option<u32>,
    /// Its list: the index of its `Start(List)` among the note's lists, in document order.
    pub list: u32,
}

/// Fills a `Structure` from the parse's events, one call per element it opens.
#[derive(Default)]
pub(crate) struct Collector {
    structure: Structure,
    /// Each open item's index and range end, innermost last.
    items: Vec<(u32, usize)>,
    /// Each open list's index and range end, innermost last.
    lists: Vec<(u32, usize)>,
    /// The lists opened so far.
    opened: u32,
}

impl Collector {
    pub(crate) fn start(&mut self, text: &str, tag: &Tag<'_>, range: &Range<usize>) {
        match tag {
            // A range can start on the line before; its marker cannot.
            Tag::List(_) => {
                close(&mut self.lists, marker_start(text, range.start));
                self.lists.push((self.opened, range.end));
                self.opened += 1;
            }
            Tag::Item => {
                let marker = marker_start(text, range.start);
                close(&mut self.items, marker);
                close(&mut self.lists, marker);
                let index = u32::try_from(self.structure.items.len()).unwrap_or(u32::MAX);
                self.structure.items.push(Item {
                    marker: ByteOffset::new(marker),
                    end: ByteOffset::new(without_line_ending(text, range).end),
                    parent: self.items.last().map(|&(parent, _)| parent),
                    list: self.lists.last().map_or(0, |&(list, _)| list),
                });
                self.items.push((index, range.end));
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

    /// Each item so far, in document order.
    pub(crate) fn items(&self) -> &[Item] {
        &self.structure.items
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
    collector
        .structure
        .items
        .last()
        .map(|item| item.marker.get())
}

/// Drops the open elements that end at or before `at`.
fn close(open: &mut Vec<(u32, usize)>, at: usize) {
    while open.last().is_some_and(|&(_, end)| end <= at) {
        open.pop();
    }
}

#[cfg(test)]
mod tests;
