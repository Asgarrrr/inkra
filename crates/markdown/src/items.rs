//! List items in the gutter (task boxes spec, D1, D11): each item's marker as the parse decides it,
//! each list's width key, and the walk that gives every line a gutter item holds its item part.

use std::iter::Peekable;
use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Tag, TagEnd};

use crate::gutters::{Gutter, ItemPart, Task, WidthKey, gutter_at, quote_gutters};
use crate::spans::{line_start, to_offsets};
use crate::{Conceal, Span, Styles};

/// A gutter item as the parse met it.
struct Record {
    /// Its range's end: it holds the lines that start before it.
    end: usize,
    /// Its index in `structure.items`, which holds its marker.
    marker: usize,
    /// Its list's index in `Items::numbers`; `None` in a bullet list.
    list: Option<usize>,
    task: Option<Task>,
}

/// The open item whose marker waits for its next event.
struct Pending {
    /// Its range's start.
    start: usize,
    /// Its index in the open elements.
    index: usize,
    /// Its index in `Items::records` when it is a gutter item.
    record: Option<usize>,
}

/// Each list item's marker, decided at the item's first event that is not one `Start(Paragraph)`
/// (markdown blocks spec, S3 correction 7): a gutter item's box, or else the `•` of any other
/// bullet. No paragraph opens right inside another, so skipping every `Start(Paragraph)` skips one.
#[derive(Default)]
pub(crate) struct Items {
    pending: Option<Pending>,
    /// The items opened so far: the next one's index in `structure.items`.
    count: usize,
    records: Vec<Record>,
    /// Each ordered list's width key, in document order.
    numbers: Vec<WidthKey>,
    /// Each open list's index in `numbers`, `None` for a bullet list, innermost last.
    open: Vec<Option<usize>>,
    bullets: Vec<Conceal>,
    /// Each line holding a task with no box, in line order, without its `\n`.
    unboxed: Vec<Range<usize>>,
}

impl Items {
    /// Called with every event, in order, before the event's element opens: `opened` elements are
    /// open, and an item this event opens is a gutter item when `gutter`. Returns the index of a
    /// checked task item that gets a box, so it and the elements above it take `DONE` (task boxes
    /// spec, D7).
    pub(crate) fn event(
        &mut self,
        text: &str,
        event: &Event<'_>,
        range: &Range<usize>,
        opened: usize,
        gutter: bool,
    ) -> Option<usize> {
        let mut done = None;
        if let Some(pending) = self.pending.take() {
            let marker = marker_start(text, pending.start);
            match (event, pending.record) {
                (Event::Start(Tag::Paragraph), _) => self.pending = Some(pending),
                // The range may start with spaces; the checkbox is its last three bytes.
                (Event::TaskListMarker(checked), Some(record)) => {
                    let checkbox = range.end - 3..range.end;
                    // pulldown-cmark scans a task marker only on its item's marker line (D1).
                    debug_assert!(
                        !text[marker..checkbox.start].contains('\n'),
                        "a checkbox off its marker's line"
                    );
                    self.records[record].task = Some(Task {
                        checkbox: to_offsets(checkbox),
                        checked: *checked,
                    });
                    done = checked.then_some(pending.index);
                }
                (Event::TaskListMarker(_), None) => {
                    self.bullets.extend(bullet(text, marker));
                    let line = line_start(text, marker);
                    let end = text[line..].find('\n').map_or(text.len(), |len| line + len);
                    self.unboxed.push(line..end);
                }
                (_, None) => self.bullets.extend(bullet(text, marker)),
                (_, Some(_)) => {}
            }
        }
        match event {
            Event::Start(Tag::List(first)) => {
                self.open.push(first.map(|_| self.numbers.len()));
                if first.is_some() {
                    self.numbers.push(WidthKey {
                        digits: 0,
                        delimiter: b'.',
                    });
                }
            }
            Event::End(TagEnd::List(_)) => {
                self.open.pop();
            }
            Event::Start(Tag::Item) => {
                let list = self.open.last().copied().flatten();
                if let Some(list) = list {
                    let marker = marker_start(text, range.start);
                    let end = marker_end(text, marker) - 1;
                    let key = &mut self.numbers[list];
                    // `CommonMark` caps a number at nine digits.
                    key.digits = key
                        .digits
                        .max(u8::try_from(end - marker).unwrap_or(u8::MAX));
                    key.delimiter = text.as_bytes()[end];
                }
                let record = gutter.then(|| {
                    self.records.push(Record {
                        end: range.end,
                        marker: self.count,
                        list,
                        task: None,
                    });
                    self.records.len() - 1
                });
                self.pending = Some(Pending {
                    start: range.start,
                    index: opened,
                    record,
                });
                self.count += 1;
            }
            _ => {}
        }
        done
    }

    /// The gutters, each held line's item part after its quote part, the width keys their item
    /// parts index, the bullets' conceals, and the lines holding a task with no box. `quotes` are
    /// the leading quotes' ranges, `items` every item's marker and `blocks` every code block's
    /// range without its last `\n`, all in document order; `spans` are the parse's. One walk, after
    /// the parse's one pass, so every width key is final (D1).
    pub(crate) fn gutters(
        self,
        text: &str,
        quotes: &[Range<usize>],
        items: &[ByteOffset],
        blocks: impl Iterator<Item = Range<usize>>,
        spans: &[Span],
    ) -> (Vec<Gutter>, Vec<WidthKey>, Vec<Conceal>, Vec<Range<usize>>) {
        let quote_lines = quote_gutters(text, quotes);
        let mut walk = Walk {
            text,
            spans,
            out: Vec::with_capacity(quote_lines.len() + self.records.len()),
            quote_lines: quote_lines.into_iter().peekable(),
            blocks: blocks.peekable(),
            held: Vec::new(),
            keys: Vec::new(),
        };
        let mut rest = self.records.as_slice();
        // The first line past the last marker line.
        let mut next = 0;
        while let Some(first) = rest.first() {
            let marker = items[first.marker].get();
            let line = line_start(text, marker);
            let on_line = |item: &&Record| line_start(text, items[item.marker].get()) == line;
            // `first` opens on the line, so `opening` holds at least it.
            let count = 1 + rest[1..].iter().take_while(on_line).count();
            let (opening, after) = rest.split_at(count);
            let last = &opening[count - 1];
            rest = after;
            walk.held_lines(&self, next, line);
            walk.release(marker);
            let mut gutter = walk.quote_line(line).unwrap_or_else(|| gutter_at(line, 0));
            // A marker line opens an item, so it is never lazy (D10).
            debug_assert_eq!(
                gutter.quotes.len(),
                gutter.depth,
                "a marker line misses a `>`"
            );
            // pulldown-cmark opens a list item mid-line only after container markers (D10).
            debug_assert!(
                !text[gutter.range.end.get()..marker].contains('>'),
                "a `>` between the quote part and a marker"
            );
            let item = self.item_part(gutter.range.end, &walk.held, opening, &mut walk.keys);
            let end = item.task.as_ref().map_or_else(
                || marker_end(text, items[last.marker].get()),
                |task| task.checkbox.end.get(),
            );
            let blank = matches!(text.as_bytes().get(end), Some(b' ' | b'\t'));
            let end = end + usize::from(blank);
            gutter.range.end = ByteOffset::new(end);
            gutter.item = Some(item);
            walk.out.push(gutter);
            let hang = column_after(0, &text.as_bytes()[line..end]);
            walk.held
                .extend(opening.iter().map(|record| Held { record, hang }));
            next = text.as_bytes()[end..]
                .iter()
                .position(|&byte| byte == b'\n')
                .map_or(text.len(), |len| end + len + 1);
        }
        walk.held_lines(&self, next, text.len());
        walk.out.extend(walk.quote_lines);
        (walk.out, walk.keys, self.bullets, self.unboxed)
    }

    /// The item part from `start` of a line that `held` hold and on which `opening` open; its
    /// width keys go to the end of `keys`.
    fn item_part(
        &self,
        start: ByteOffset,
        held: &[Held<'_>],
        opening: &[Record],
        keys: &mut Vec<WidthKey>,
    ) -> ItemPart {
        // An ordered task shows its number and its box; any other item one mark (D11).
        let columns = |item: &Record| 1 + usize::from(item.list.is_some() && item.task.is_some());
        let first = keys.len();
        keys.extend(
            held.iter()
                .map(|held| held.record)
                .chain(opening)
                .filter_map(|item| item.list.map(|list| self.numbers[list])),
        );
        ItemPart {
            start,
            indent: held.iter().map(|held| columns(held.record)).sum(),
            numbers: first..keys.len(),
            task: opening.last().and_then(|item| item.task.clone()),
        }
    }
}

/// A gutter item holding the lines the walk visits.
struct Held<'r> {
    record: &'r Record,
    /// The column after its marker line's item part (D11).
    hang: usize,
}

/// The walk's cursors, each moving forward only: the quote lines not yet given out, the code
/// blocks not yet passed, the gutter items holding the line, outermost first, and the gutters and
/// width keys so far.
struct Walk<'t, 'r, B: Iterator<Item = Range<usize>>> {
    text: &'t str,
    spans: &'t [Span],
    quote_lines: Peekable<std::vec::IntoIter<Gutter>>,
    blocks: Peekable<B>,
    held: Vec<Held<'r>>,
    out: Vec<Gutter>,
    keys: Vec<WidthKey>,
}

impl<B: Iterator<Item = Range<usize>>> Walk<'_, '_, B> {
    /// Drops the items that end by `at`.
    fn release(&mut self, at: usize) {
        while self.held.last().is_some_and(|held| held.record.end <= at) {
            self.held.pop();
        }
    }

    /// Gives out the quote lines before the line start `line`, and takes the one at it.
    fn quote_line(&mut self, line: usize) -> Option<Gutter> {
        let quote_lines = &mut self.quote_lines;
        self.out.extend(std::iter::from_fn(|| {
            quote_lines.next_if(|gutter| gutter.range.start.get() < line)
        }));
        self.quote_lines
            .next_if(|gutter| gutter.range.start.get() == line)
    }

    /// The lines from the line start `from` to `to` that an item holds, where no item opens: a
    /// code block's line hides its blanks before its fence or code text, any other its blanks below
    /// the innermost item's hang column. A blank line takes no item part, unless a code block holds it (D11).
    /// Stops where no item holds the line.
    fn held_lines(&mut self, items: &Items, from: usize, to: usize) {
        let text = self.text;
        let bytes = text.as_bytes();
        let mut next = from;
        // Holders are released and checked first: an empty hold searches no line end (D1).
        while next < to {
            let line = next;
            self.release(line);
            let Some(&Held { hang, .. }) = self.held.last() else {
                return;
            };
            let end = bytes[line..]
                .iter()
                .position(|&byte| byte == b'\n')
                .map_or(text.len(), |len| line + len);
            next = end + 1;
            let quoted = self.quote_line(line);
            let start = quoted
                .as_ref()
                .map_or(line, |gutter| gutter.range.end.get());
            let is_blank = |byte: &u8| matches!(byte, b' ' | b'\t');
            let in_block = self.in_block(line, end);
            if bytes[start..end].iter().all(is_blank) && !in_block {
                self.out.extend(quoted);
                continue;
            }
            let block = in_block
                .then(|| block_start(self.spans, line, end))
                .flatten();
            let blanks = if let Some(block) = block {
                bytes[start..block]
                    .iter()
                    .take_while(|byte| is_blank(byte))
                    .count()
            } else {
                let column = column_after(0, &bytes[line..start]);
                bytes[start..end]
                    .iter()
                    .scan(column, |column, byte| {
                        let at = *column;
                        *column = column_after(at, &[*byte]);
                        Some((at, byte))
                    })
                    .take_while(|&(at, byte)| at < hang && is_blank(byte))
                    .count()
            };
            let mut gutter = quoted.unwrap_or_else(|| gutter_at(line, 0));
            gutter.item =
                Some(items.item_part(ByteOffset::new(start), &self.held, &[], &mut self.keys));
            gutter.range.end = ByteOffset::new(start + blanks);
            self.out.push(gutter);
        }
    }

    /// Whether a code block holds the line from `line` to `end`; lines come in order.
    fn in_block(&mut self, line: usize, end: usize) -> bool {
        while self.blocks.next_if(|block| block.end < line).is_some() {}
        self.blocks.peek().is_some_and(|block| block.start <= end)
    }
}

/// The first byte of the line from `line` to `end` that a code block styles, its fence or its code
/// text: pulldown-cmark starts it past the container's indent and the fence's (D11).
fn block_start(spans: &[Span], line: usize, end: usize) -> Option<usize> {
    let first = spans.partition_point(|span| span.range.end.get() <= line);
    spans[first..]
        .iter()
        .take_while(|span| span.range.end.get() <= end)
        .find(|span| span.styles.contains(Styles::CODE_BLOCK))
        .map(|span| span.range.start.get())
}

/// An item's marker: pulldown-cmark starts its range up to a tab's leftover columns early, back
/// onto the line ending or quote marker before it, and a marker is never a blank or `>`.
pub(crate) fn marker_start(text: &str, item: usize) -> usize {
    item + text.as_bytes()[item..]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'\n' | b'>'))
        .count()
}

/// The end of the marker at `marker`: one byte for a bullet, the digits and the `.` or `)` after
/// them for a number (`CommonMark` 5.2).
pub(crate) fn marker_end(text: &str, marker: usize) -> usize {
    let digits = text.as_bytes()[marker..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit());
    marker + 1 + digits.count()
}

/// The column after `bytes` from `column`, a tab stopping at the next multiple of 4 (`CommonMark` 2.2).
pub(crate) fn column_after(column: usize, bytes: &[u8]) -> usize {
    bytes.iter().fold(column, |column, &byte| match byte {
        b'\t' => column + 4 - column % 4,
        _ => column + 1,
    })
}

/// The element is the bullet byte alone, so a caret at the item's content does not reveal it. An
/// ordered item keeps its number.
fn bullet(text: &str, marker: usize) -> Option<Conceal> {
    matches!(text.as_bytes().get(marker), Some(b'-' | b'*' | b'+')).then(|| {
        let bullet = to_offsets(marker..marker + 1);
        Conceal {
            element: bullet.clone(),
            marker: bullet,
            replacement: "•",
        }
    })
}
