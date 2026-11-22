//! List items in the gutter (task boxes spec, D1, D11): each item's marker as the parse decides it,
//! each list's width key, and the walk that gives every line a gutter item holds its item part.

use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Tag, TagEnd};

use crate::Conceal;
use crate::gutters::{Gutter, ItemPart, Task, WidthKey, gutter_at, quote_gutters};
use crate::spans::{line_end, line_start, to_offsets};
use crate::structure::Item;
use walk::{Held, Walk};

/// A gutter item as the parse met it.
struct Record {
    /// Its range's end: it holds the lines that start before it.
    end: usize,
    /// Its index in `structure.items`, which holds its marker.
    marker: usize,
    /// Its list's index in `Items::list_keys`; `None` in a bullet list.
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
    list_keys: Vec<WidthKey>,
    /// Each open list's index in `list_keys`, `None` for a bullet list, innermost last.
    open: Vec<Option<usize>>,
    bullets: Vec<Conceal>,
    /// Each line holding a task with no box, in line order, without its `\n`.
    unboxed: Vec<Range<usize>>,
    /// Whether a code or an HTML block is open.
    in_leaf: bool,
    /// Each text event of a code or an HTML block inside a list, in order (D11).
    leaf_texts: Vec<LeafText>,
}

/// A text event of a code or an HTML block: pulldown-cmark emits one or two per line, the first
/// an empty one when the line's containers leave columns to the text, a tab's or the block's own
/// indent (D11).
struct LeafText {
    start: usize,
    /// The columns before `start` that belong to the text.
    leftover: usize,
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
                    self.unboxed
                        .push(line_start(text, marker)..line_end(text, marker));
                }
                (_, None) => self.bullets.extend(bullet(text, marker)),
                (_, Some(_)) => {}
            }
        }
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => self.in_leaf = true,
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock) => self.in_leaf = false,
            // Only a gutter item's line reads one, and only an open list holds a gutter item.
            Event::Text(leaf) | Event::Html(leaf) if self.in_leaf && !self.open.is_empty() => {
                self.leaf_texts.push(LeafText {
                    start: range.start,
                    leftover: if range.is_empty() { leaf.len() } else { 0 },
                });
            }
            Event::Start(Tag::List(first)) => {
                self.open.push(first.map(|_| self.list_keys.len()));
                if first.is_some() {
                    self.list_keys.push(WidthKey {
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
                    let key = &mut self.list_keys[list];
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

    /// The gutters, each held line's item part after its quote part, and what the parse keeps
    /// beside them. `quotes` are the leading quotes' ranges, `items` every item and
    /// `blocks` every code block's range without its last `\n`, all in document order. One walk,
    /// after the parse's one pass, so every width key is final (D1).
    pub(crate) fn gutters(
        self,
        text: &str,
        quotes: &[Range<usize>],
        items: &[Item],
        blocks: impl Iterator<Item = Range<usize>>,
    ) -> ItemGutters {
        let quote_lines = quote_gutters(text, quotes);
        let mut walk = Walk {
            text,
            leaf_texts: self.leaf_texts.iter().peekable(),
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
            let marker = items[first.marker].marker.get();
            let line = line_start(text, marker);
            let on_line =
                |item: &&Record| line_start(text, items[item.marker].marker.get()) == line;
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
                || marker_end(text, items[last.marker].marker.get()),
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
            next = line_end(text, end) + 1;
        }
        walk.held_lines(&self, next, text.len());
        walk.out.extend(walk.quote_lines);
        ItemGutters {
            gutters: walk.out,
            width_keys: walk.keys,
            bullets: self.bullets,
            unboxed: self.unboxed,
        }
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
                .filter_map(|item| item.list.map(|list| self.list_keys[list])),
        );
        ItemPart {
            start,
            indent: held.iter().map(|held| columns(held.record)).sum(),
            keys: first..keys.len(),
            task: opening.last().and_then(|item| item.task.clone()),
            marked: !opening.is_empty(),
        }
    }
}

/// What the item walk gives the parse.
pub(crate) struct ItemGutters {
    /// Every line's gutter, quoted or held, sorted by line.
    pub(crate) gutters: Vec<Gutter>,
    /// The width keys the item parts index.
    pub(crate) width_keys: Vec<WidthKey>,
    /// The bullets' conceals.
    pub(crate) bullets: Vec<Conceal>,
    /// The lines holding a task with no box.
    pub(crate) unboxed: Vec<Range<usize>>,
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

/// The blanks after an item's marker, as `CommonMark` 5.2 counts them for its content start.
pub(crate) struct MarkerBlanks {
    /// The marker's end.
    pub(crate) end: usize,
    /// Their bytes, at most five.
    pub(crate) len: usize,
    /// Their columns.
    pub(crate) columns: usize,
    /// The marker end's column, counted from the line's start.
    after: usize,
}

impl MarkerBlanks {
    /// The blanks after the marker at `marker`, on the line starting at `line`.
    pub(crate) fn at(text: &str, line: usize, marker: usize) -> Self {
        let bytes = text.as_bytes();
        let end = marker_end(text, marker);
        let len = bytes[end..]
            .iter()
            .take(5)
            .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
            .count();
        let after = column_after(0, &bytes[line..end]);
        Self {
            end,
            len,
            columns: column_after(after, &bytes[end..end + len]) - after,
            after,
        }
    }

    /// The content start's column: past one to four columns of blanks, else past one.
    pub(crate) fn content_column(&self) -> usize {
        self.after
            + if (1..=4).contains(&self.columns) {
                self.columns
            } else {
                1
            }
    }
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

mod walk;
