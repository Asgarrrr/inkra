//! Gutters: each quoted line's `>` prefix, found from the parse (markdown blocks spec, D1), then
//! the item part of a line a gutter item holds (task boxes spec, D1, D11).

use std::ops::Range;

use inkra_text::{ByteOffset, Hunk};

use crate::spans::{line_ranges, line_start, push, to_offsets};
use crate::{Conceal, Markdown, Span, Styles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gutter {
    /// From the line's start through its quote part, then its item part; empty on a lazy line.
    pub range: Range<ByteOffset>,
    /// Each `>` on the line, outermost first; fewer than `depth` on a lazy line.
    pub quotes: Vec<ByteOffset>,
    /// The leading quotes that contain the line, `>` or not; 0 on an item line outside a quote.
    pub depth: usize,
    /// The bytes past the quote part on a marker line of a gutter item (D11).
    pub item: Option<ItemPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPart {
    /// Where it starts: past the quote part's last `>` and its blank, or the line's start.
    pub start: ByteOffset,
    /// Its blank slots: one per column of each gutter item holding the line whose marker stands
    /// on an earlier line (D11).
    pub indent: usize,
    /// The width key of each number slot on the line, blank or marked, in slot order; empty,
    /// and unallocated, under no ordered list (D3, Open question 10).
    pub numbers: Vec<WidthKey>,
    /// The box, when the innermost gutter item opening on the line is a task.
    pub task: Option<Task>,
}

/// What a list's number width depends on: its longest number (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidthKey {
    pub digits: u8,
    /// `.` or `)`; a new delimiter starts a new list.
    pub delimiter: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}

/// What one slot of a marker line shows besides the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemMarker {
    /// A `-`, `*` or `+`, painted as `•`.
    Bullet,
    /// A number's digits and its `.` or `)`, painted as typed.
    Number(Range<ByteOffset>),
}

impl Gutter {
    /// The quote part's end, past its last `>` and the one space or tab after it.
    #[must_use]
    pub fn quote_end(&self) -> ByteOffset {
        self.item.as_ref().map_or(self.range.end, |item| item.start)
    }
}

impl ItemPart {
    /// A marker line's marks in slot order, before the box; `part` is the item part's text, from
    /// `start` to its gutter's end. They are its runs of non-blanks before the checkbox, a number's
    /// starting with a digit, but a task's own bullet, whose slot the box takes (D10).
    pub fn marks<'p>(&self, part: &'p str) -> impl Iterator<Item = ItemMarker> + 'p {
        let start = self.start;
        let head = self
            .task
            .as_ref()
            .map_or(part, |task| &part[..task.checkbox.start - start]);
        let boxed = self.task.is_some();
        let mut runs = head
            .split([' ', '\t'])
            .scan(0, |at, run| {
                let from = *at;
                *at += run.len() + 1;
                Some((from, run))
            })
            .filter(|(_, run)| !run.is_empty())
            .peekable();
        std::iter::from_fn(move || {
            loop {
                let (at, run) = runs.next()?;
                if run.starts_with(|c: char| c.is_ascii_digit()) {
                    let at = start + at;
                    return Some(ItemMarker::Number(at..at + run.len()));
                }
                if !boxed || runs.peek().is_some() {
                    return Some(ItemMarker::Bullet);
                }
            }
        })
    }
}

impl Markdown {
    /// The task whose gutter starts at the line start `line_start` (D1).
    #[must_use]
    pub fn task_at(&self, line_start: ByteOffset) -> Option<&Task> {
        let at = self
            .gutters
            .partition_point(|gutter| gutter.range.start < line_start);
        self.gutters
            .get(at)
            .filter(|gutter| gutter.range.start == line_start)?
            .item
            .as_ref()?
            .task
            .as_ref()
    }
}

impl Task {
    /// Ticks or clears it by its middle byte alone, so `[X]` clears as `[x]` does (D5).
    #[must_use]
    pub fn flip(&self) -> Hunk {
        let middle = self.checkbox.start + 1;
        Hunk {
            range: middle..middle + 1,
            text: if self.checked { " " } else { "x" }.to_owned(),
        }
    }
}

/// One gutter per line inside a leading quote; `quotes` holds their ranges, without a trailing
/// `\n`, in start order.
pub(crate) fn quote_gutters(text: &str, quotes: &[Range<usize>]) -> Vec<Gutter> {
    let mut gutters = Vec::new();
    // The end of each quote open on this line.
    let mut open: Vec<usize> = Vec::new();
    let mut rest = quotes;
    let Some(first) = rest.first() else {
        return gutters;
    };
    let first = line_start(text, first.start);
    for Range { start, end } in line_ranges(text, first) {
        let entering = rest.iter().take_while(|quote| quote.start <= end).count();
        if open.is_empty() && entering == 0 {
            if rest.is_empty() {
                break;
            }
            continue;
        }
        open.extend(rest[..entering].iter().map(|quote| quote.end));
        rest = &rest[entering..];
        gutters.push(gutter(text, start..end, open.len()));
        open.retain(|&quote_end| end < quote_end);
    }
    gutters
}

pub(crate) fn gutter_at(start: usize, depth: usize) -> Gutter {
    Gutter {
        range: to_offsets(start..start),
        quotes: Vec::new(),
        depth,
        item: None,
    }
}

/// One `>` per open quote, outermost first. Blanks count in columns as pulldown-cmark's `LineStart`
/// counts them (D1): a `>` takes up to three columns before it, entering a tab whole
/// (`parse.rs:1515-1518`); a quote opens only within three (`firstpass.rs:109-114`), where the same
/// scan finds it. The first `>` missing makes the line lazy from there on.
fn gutter(text: &str, line: Range<usize>, depth: usize) -> Gutter {
    let mut blanks = Blanks {
        bytes: &text.as_bytes()[line.clone()],
        at: 0,
        tab_stop: 0,
        left: 0,
    };
    let mut gutter = gutter_at(line.start, depth);
    for _ in 0..depth {
        let save = blanks;
        blanks.scan(3);
        if blanks.bytes.get(blanks.at) != Some(&b'>') {
            blanks = save;
            break;
        }
        gutter.quotes.push(ByteOffset::new(line.start + blanks.at));
        blanks.at += 1;
        blanks.scan(1);
    }
    gutter.range.end = ByteOffset::new(line.start + blanks.at);
    gutter
}

/// A line's leading blanks as `LineStart` scans them
/// (`pulldown-cmark-0.13.6/src/scanners.rs:160-187`).
#[derive(Clone, Copy)]
struct Blanks<'b> {
    bytes: &'b [u8],
    at: usize,
    /// The byte after the last tab scanned, which lies on a tab stop.
    tab_stop: usize,
    /// The columns of a scanned tab not yet taken, before `at`.
    left: usize,
}

impl Blanks<'_> {
    /// Takes up to `columns` columns of blanks.
    fn scan(&mut self, columns: usize) {
        let taken = self.left.min(columns);
        self.left -= taken;
        let mut columns = columns - taken;
        while columns > 0 {
            match self.bytes.get(self.at) {
                Some(b' ') => {
                    self.at += 1;
                    columns -= 1;
                }
                Some(b'\t') => {
                    let width = 4 - (self.at - self.tab_stop) % 4;
                    self.at += 1;
                    self.tab_stop = self.at;
                    let taken = width.min(columns);
                    columns -= taken;
                    self.left = width - taken;
                }
                _ => break,
            }
        }
    }
}

/// One conceal per `>`, all with the element from the gutter's start through its last `>`, so a
/// line's `>`s reveal together and a caret after the last one's space reveals none. An item part's
/// conceal follows, so the two parts reveal apart: its element and marker run through its last
/// mark, or over the whole part when it has none, and an empty part has none (D2).
pub(crate) fn gutter_conceals<'g>(
    text: &'g str,
    gutters: &'g [Gutter],
) -> impl Iterator<Item = Conceal> + 'g {
    gutters.iter().flat_map(move |gutter| {
        let last = gutter
            .quotes
            .last()
            .map_or(gutter.range.start, |&last| last + 1);
        let element = gutter.range.start..last;
        let quotes = gutter.quotes.iter().map(move |&quote| Conceal {
            element: element.clone(),
            marker: quote..quote + 1,
            replacement: "",
        });
        let item = gutter.item.as_ref().and_then(|item| {
            let part = &text[item.start.get()..gutter.range.end.get()];
            let marked = part.trim_end_matches([' ', '\t']).len();
            let end = item.start + if marked == 0 { part.len() } else { marked };
            (item.start < end).then_some(Conceal {
                element: item.start..end,
                marker: item.start..end,
                replacement: "",
            })
        });
        quotes.chain(item)
    })
}

/// A gutter's quote part is styled by the gutter alone: every span is cut around it, whatever
/// element emitted it. A leading quote's styles are `QUOTE`, its ancestors being quotes. An item
/// part keeps its gap style; no inline element crosses an item's start (S3 correction 11).
pub(crate) fn cut(text: &str, spans: &mut Vec<Span>, gutters: &[Gutter]) {
    // A gutter span must stay on one line, as every span does.
    debug_assert!(
        gutters
            .iter()
            .all(|gutter| !text[gutter.range.start.get()..gutter.range.end.get()].contains('\n')),
        "a gutter crosses a line"
    );
    // A lazy line or an item outside a quote has no quote part; a note with none keeps its spans.
    let mut gutters = gutters
        .iter()
        .map(|gutter| gutter.range.start.get()..gutter.quote_end().get())
        .filter(|part| !part.is_empty())
        .peekable();
    if gutters.peek().is_none() {
        return;
    }
    let marker = Styles::QUOTE | Styles::MARKER;
    // Everything before this byte is a gutter's or already re-emitted.
    let mut cut = 0;
    for span in std::mem::take(spans) {
        let range = span.range.start.get()..span.range.end.get();
        while let Some(gutter) = gutters.next_if(|gutter| gutter.start < range.end) {
            push(spans, range.start.max(cut)..gutter.start, span.styles);
            cut = gutter.end;
            push(spans, gutter, marker);
        }
        push(spans, range.start.max(cut)..range.end, span.styles);
    }
    for gutter in gutters {
        push(spans, gutter, marker);
    }
}
