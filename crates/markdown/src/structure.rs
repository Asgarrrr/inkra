//! What `⌘L` reads of the parse: where items and quotes open, and the lines it skips (task boxes
//! spec, D1, D6).

use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::items::{column_after, marker_start};
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
    /// Its parent item's index in `Structure::items`, as `u32` to keep `Item` at 32 bytes.
    parent: Option<u32>,
    /// Its list's index, as `u32` for the same reason.
    list: u32,
}

impl Item {
    /// Its parent item's index in `Structure::items`; `None` for a top-level item.
    #[must_use]
    pub fn parent(&self) -> Option<usize> {
        self.parent.map(|parent| parent as usize)
    }

    /// Its list: the index of its `Start(List)` among the note's lists, in document order.
    #[must_use]
    pub fn list(&self) -> usize {
        self.list as usize
    }
}

/// An item or a list the walk is inside: its index, and its range's end.
struct Open {
    index: u32,
    end: usize,
}

/// Fills a `Structure` from the parse's events, one call per element it opens.
#[derive(Default)]
pub(crate) struct Collector {
    structure: Structure,
    /// The open items, innermost last.
    items: Vec<Open>,
    /// The open lists, innermost last.
    lists: Vec<Open>,
    /// The lists opened so far.
    opened: u32,
}

impl Collector {
    pub(crate) fn start(&mut self, text: &str, tag: &Tag<'_>, range: &Range<usize>) {
        match tag {
            // A range can start on the line before; its marker cannot.
            Tag::List(_) => {
                close(&mut self.lists, marker_start(text, range.start));
                self.lists.push(Open {
                    index: self.opened,
                    end: range.end,
                });
                self.opened += 1;
            }
            Tag::Item => {
                let marker = marker_start(text, range.start);
                close(&mut self.items, marker);
                close(&mut self.lists, marker);
                #[expect(
                    clippy::expect_used,
                    reason = "every item but the last takes two bytes: 2^32 items need 8 GiB"
                )]
                let index = u32::try_from(self.structure.items.len())
                    .expect("a note holds fewer than 2^32 items");
                self.structure.items.push(Item {
                    marker: ByteOffset::new(marker),
                    end: ByteOffset::new(without_line_ending(text, range).end),
                    parent: self.items.last().map(|open| open.index),
                    list: self.lists.last().map_or(0, |open| open.index),
                });
                self.items.push(Open {
                    index,
                    end: range.end,
                });
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

/// A line where no item opens, read alone from `from` by one pass. Its text follows its columns
/// before `from` mod 4 in spaces, so its tabs stop as in the line and the pad opens no code (D6).
pub(crate) struct Fallback {
    /// The pad, the line's text from `from`, and a newline.
    pub(crate) piece: String,
    /// The innermost item's marker in `piece`.
    pub(crate) marker: usize,
    from: usize,
    pad: usize,
}

impl Fallback {
    /// The pass over `line` from `from`; `None` when it opens no item.
    pub(crate) fn read(text: &str, line: &Range<usize>, from: usize) -> Option<Self> {
        #[cfg(test)]
        FALLBACKS.set(FALLBACKS.get() + 1);
        let pad = column_after(0, &text.as_bytes()[line.start..from]) % 4;
        let piece = format!("{:pad$}{}\n", "", &text[from..line.end]);
        let marker = innermost_item(&piece)?;
        Some(Self {
            piece,
            marker,
            from,
            pad,
        })
    }

    /// `at`, an offset in `piece` past its pad, in the text.
    pub(crate) fn in_text(&self, at: usize) -> usize {
        self.from + at - self.pad
    }
}

/// The marker of the innermost item one pass reads in `text`, a single line, as `parse` reads it:
/// `text` ends with a newline.
fn innermost_item(text: &str) -> Option<usize> {
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
fn close(open: &mut Vec<Open>, at: usize) {
    while open.last().is_some_and(|open| open.end <= at) {
        open.pop();
    }
}

#[cfg(test)]
thread_local! {
    /// The fallback passes run, for the tests that bound them to one per command.
    pub(crate) static FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
