//! Gutters: each quoted line's `>` prefix, found from the parse (markdown blocks spec, D1), and a
//! task item's markers through its checkbox when it gets a box (task boxes spec, D1, D10).

use std::ops::Range;

use inkra_text::{ByteOffset, Hunk};
use pulldown_cmark::{Event, Tag};

use crate::spans::{line_ranges, line_start, push, to_offsets};
use crate::{Conceal, Markdown, Span, Styles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gutter {
    /// From the line's start through its last `>` and the one space or tab after it, or through
    /// its task's checkbox and the one space or tab after that; empty on a lazy line.
    pub range: Range<ByteOffset>,
    /// Each `>` on the line, outermost first; fewer than `depth` on a lazy line.
    pub quotes: Vec<ByteOffset>,
    /// The leading quotes that contain the line, `>` or not; 0 on a task line outside a quote.
    pub depth: usize,
    /// The task the line starts, when it gets a box: the line holds all its `>` (D10).
    pub task: Option<Task>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Where the task part starts: past the quote part's last `>` and its blank.
    pub start: ByteOffset,
    /// Where the box run starts: the bullet, or an ordered task's checkbox.
    pub run: ByteOffset,
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}

impl Gutter {
    /// The quote part's end, past its last `>` and the one space or tab after it.
    #[must_use]
    pub fn quote_end(&self) -> ByteOffset {
        self.task.as_ref().map_or(self.range.end, |task| task.start)
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

/// Each list item's marker, decided at the item's first event that is not one `Start(Paragraph)`
/// (markdown blocks spec, S3 correction 7): a task, or else a bullet's `•`. No paragraph opens
/// right inside another, so skipping every `Start(Paragraph)` skips one.
#[derive(Default)]
pub(crate) struct Items {
    /// The open item's start and its index in the open elements.
    pending: Option<(usize, usize)>,
    /// Each task gutter, in line order.
    tasks: Vec<Gutter>,
    bullets: Vec<Conceal>,
    /// Each line holding a task with no box, in line order, without its `\n`.
    unboxed: Vec<Range<usize>>,
}

impl Items {
    /// Called with every event, in order, before the event's element opens: `opened` elements are
    /// open, the first `depth` of them leading quotes. Returns the index of a checked task item
    /// that gets a gutter, so it and the elements above it take `DONE` (task boxes spec, D7).
    pub(crate) fn event(
        &mut self,
        text: &str,
        event: &Event<'_>,
        range: &Range<usize>,
        opened: usize,
        depth: usize,
    ) -> Option<usize> {
        let mut done = None;
        if let Some((item, index)) = self.pending.take() {
            let marker = marker_start(text, item);
            match event {
                Event::Start(Tag::Paragraph) => self.pending = Some((item, index)),
                // The range may start with spaces; the checkbox is its last three bytes.
                Event::TaskListMarker(checked) => {
                    let checkbox = range.end - 3..range.end;
                    let line = line_start(text, marker);
                    let line = line..text[line..].find('\n').map_or(text.len(), |len| line + len);
                    if let Some(gutter) =
                        task_gutter(text, line.clone(), marker, checkbox, *checked, depth)
                    {
                        done = checked.then_some(index);
                        self.tasks.push(gutter);
                    } else {
                        self.bullets.extend(bullet(text, marker));
                        self.unboxed.push(line);
                    }
                }
                _ => self.bullets.extend(bullet(text, marker)),
            }
        }
        if let Event::Start(Tag::Item) = event {
            self.pending = Some((range.start, opened));
        }
        done
    }

    /// The gutters, each task gutter in place of its line's quote gutter, the bullets' conceals, and
    /// the lines holding a task with no box.
    pub(crate) fn gutters(
        self,
        text: &str,
        quotes: &[Range<usize>],
    ) -> (Vec<Gutter>, Vec<Conceal>, Vec<Range<usize>>) {
        let mut quote_gutters = gutters(text, quotes).into_iter().peekable();
        let mut out = Vec::with_capacity(quote_gutters.len() + self.tasks.len());
        for task in self.tasks {
            let line = task.range.start;
            out.extend(std::iter::from_fn(|| {
                quote_gutters.next_if(|gutter| gutter.range.start < line)
            }));
            let replaced = quote_gutters.next_if(|gutter| gutter.range.start == line);
            debug_assert!(
                replaced.is_none_or(|replaced| replaced.quote_end() == task.quote_end()),
                "a task's quote part differs from its line's"
            );
            out.push(task);
        }
        out.extend(quote_gutters);
        (out, self.bullets, self.unboxed)
    }
}

/// A task gets a box when its line holds all its `>` and none stands between the quote part and its
/// marker: a list item opens mid-line only after container markers, so the lead holds blanks and
/// the markers of items opening on the line (D10). Any other keeps its `•`.
fn task_gutter(
    text: &str,
    line: Range<usize>,
    marker: usize,
    checkbox: Range<usize>,
    checked: bool,
    depth: usize,
) -> Option<Gutter> {
    // pulldown-cmark scans a task marker only on its item's marker line (D1).
    debug_assert!(
        !text[marker..checkbox.start].contains('\n'),
        "a checkbox off its marker's line"
    );
    let mut gutter = gutter(text, line, depth);
    let start = gutter.range.end;
    let boxed = gutter.quotes.len() == gutter.depth
        && text
            .get(start.get()..marker)
            .is_some_and(|lead| !lead.contains('>'));
    boxed.then(|| {
        let blank = matches!(text.as_bytes().get(checkbox.end), Some(b' ' | b'\t'));
        gutter.range.end = ByteOffset::new(checkbox.end + usize::from(blank));
        let run = if is_bullet(text, marker) {
            marker
        } else {
            checkbox.start
        };
        gutter.task = Some(Task {
            start,
            run: ByteOffset::new(run),
            checkbox: to_offsets(checkbox),
            checked,
        });
        gutter
    })
}

/// An item's marker: pulldown-cmark starts its range up to a tab's leftover columns early, back
/// onto the line ending or quote marker before it, and a marker is never a blank or `>`.
pub(crate) fn marker_start(text: &str, item: usize) -> usize {
    item + text.as_bytes()[item..]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'\n' | b'>'))
        .count()
}

/// An ordered item's marker starts with a digit (`CommonMark` 5.2), a bullet item's does not.
fn is_bullet(text: &str, marker: usize) -> bool {
    matches!(text.as_bytes().get(marker), Some(b'-' | b'*' | b'+'))
}

/// The element is the bullet byte alone, so a caret at the item's content does not reveal it. An
/// ordered item keeps its number.
fn bullet(text: &str, marker: usize) -> Option<Conceal> {
    is_bullet(text, marker).then(|| {
        let bullet = to_offsets(marker..marker + 1);
        Conceal {
            element: bullet.clone(),
            marker: bullet,
            replacement: "•",
        }
    })
}

/// One gutter per line inside a leading quote; `quotes` holds their ranges, without a trailing
/// `\n`, in start order.
fn gutters(text: &str, quotes: &[Range<usize>]) -> Vec<Gutter> {
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

fn gutter_at(start: usize, depth: usize) -> Gutter {
    Gutter {
        range: to_offsets(start..start),
        quotes: Vec::new(),
        depth,
        task: None,
    }
}

/// One `>` per open quote, outermost first, each after up to three spaces (`CommonMark` 5.1);
/// the first one missing makes the line lazy from there on.
fn gutter(text: &str, line: Range<usize>, depth: usize) -> Gutter {
    let bytes = &text.as_bytes()[line.clone()];
    let mut gutter = gutter_at(line.start, depth);
    let mut end = 0;
    for _ in 0..depth {
        let indent = bytes[end..]
            .iter()
            .take(3)
            .take_while(|&&byte| byte == b' ')
            .count();
        if bytes.get(end + indent) != Some(&b'>') {
            break;
        }
        gutter
            .quotes
            .push(ByteOffset::new(line.start + end + indent));
        end += indent + 1;
        end += usize::from(matches!(bytes.get(end), Some(b' ' | b'\t')));
    }
    gutter.range.end = ByteOffset::new(line.start + end);
    gutter
}

/// One conceal per `>`, all with the element from the gutter's start through its last `>`, so a
/// line's `>`s reveal together and a caret after the last one's space reveals none. A task's
/// conceal follows, its element the task part, so the two parts reveal apart (D2), and its marker
/// the box run: the lead's markers keep their own conceals (D10).
pub(crate) fn gutter_conceals(gutters: &[Gutter]) -> impl Iterator<Item = Conceal> + '_ {
    gutters.iter().flat_map(|gutter| {
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
        let task = gutter.task.as_ref().map(|task| Conceal {
            element: task.start..task.checkbox.end,
            marker: task.run..task.checkbox.end,
            replacement: "",
        });
        quotes.chain(task)
    })
}

/// A gutter's quote part is styled by the gutter alone: every span is cut around it, whatever
/// element emitted it. A leading quote's styles are `QUOTE`, its ancestors being quotes. A task
/// part keeps its gap style; no inline element crosses an item's start (S3 correction 11).
pub(crate) fn cut(text: &str, spans: &mut Vec<Span>, gutters: &[Gutter]) {
    // A gutter span must stay on one line, as every span does.
    debug_assert!(
        gutters
            .iter()
            .all(|gutter| !text[gutter.range.start.get()..gutter.range.end.get()].contains('\n')),
        "a gutter crosses a line"
    );
    // A lazy line or a task outside a quote has no quote part; a note with none keeps its spans.
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
