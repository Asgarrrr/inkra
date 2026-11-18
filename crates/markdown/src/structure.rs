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
    /// Each list item that opens, in document order.
    pub items: Vec<ListItem>,
    /// Each block quote's start, where pulldown-cmark starts its range, in document order.
    pub quotes: Vec<ByteOffset>,
    /// Each code block, front matter, table, HTML block and ATX heading, from its first line's
    /// start to its last line's end, and each line holding a task with no box. Sorted; none holds
    /// another.
    pub skipped: Vec<Range<ByteOffset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The bullet, or the digits and their `.` or `)`.
    pub marker: Range<ByteOffset>,
    /// The content start (D6); the marker's end when the line ends there.
    pub content: ByteOffset,
}

/// Fills a `Structure` from the parse's events, one call per element it opens.
#[derive(Default)]
pub(crate) struct Collector {
    structure: Structure,
    /// The last item's marker and its column: the next marker on its line counts from there.
    carry: (usize, usize),
}

impl Collector {
    pub(crate) fn start(&mut self, text: &str, tag: &Tag<'_>, range: &Range<usize>) {
        match tag {
            Tag::Item => {
                let item = self.item(text, marker_start(text, range.start));
                self.structure.items.push(item);
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

    /// The item whose marker starts at `marker`: its column carried from the last marker on its
    /// line, so a line's prefix is read once, then up to five blanks after it (D6).
    fn item(&mut self, text: &str, marker: usize) -> ListItem {
        let bytes = text.as_bytes();
        let (mut from, mut column) = self.carry;
        if let Some(newline) = text[from..marker].rfind('\n') {
            (from, column) = (from + newline + 1, 0);
        }
        let column = column_after(column, &bytes[from..marker]);
        self.carry = (marker, column);
        // A bullet is one byte, a number its digits and the `.` or `)` after them.
        let end = marker
            + 1
            + bytes[marker..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
        let blanks = bytes[end..]
            .iter()
            .take(5)
            .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
            .count();
        let after = column + end - marker;
        let content = match column_after(after, &bytes[end..end + blanks]) - after {
            0 => end,
            1..=4 => end + blanks,
            _ => end + 1,
        };
        ListItem {
            marker: to_offsets(marker..end),
            content: ByteOffset::new(content),
        }
    }
}

/// The first item one pass reads in `text`, as `parse` reads it: `text` ends with a newline.
pub(crate) fn first_item(text: &str) -> Option<ListItem> {
    let (_, range) = Parser::new_ext(text, options())
        .into_offset_iter()
        .find(|(event, _)| matches!(event, Event::Start(Tag::Item)))?;
    Some(Collector::default().item(text, marker_start(text, range.start)))
}

/// The column after `bytes` from `column`, a tab stopping at the next multiple of 4 (`CommonMark` 2.2).
pub(crate) fn column_after(column: usize, bytes: &[u8]) -> usize {
    bytes.iter().fold(column, |column, &byte| match byte {
        b'\t' => column + 4 - column % 4,
        _ => column + 1,
    })
}

#[cfg(test)]
mod tests;
