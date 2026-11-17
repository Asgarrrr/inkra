//! Gutters: each quoted line's `>` prefix, found from the parse (markdown blocks spec, D1), and a
//! task item's marker through its checkbox when it leads its line (task boxes spec, D1).

use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Tag};

use crate::spans::{push, to_offsets};
use crate::{Conceal, Span, Styles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gutter {
    /// From the line's start through its last `>` and the one space or tab after it, or through
    /// its task's checkbox and the one space or tab after that; empty on a lazy line.
    pub range: Range<ByteOffset>,
    /// Each `>` on the line, outermost first; fewer than `depth` on a lazy line.
    pub quotes: Vec<ByteOffset>,
    /// The leading quotes that contain the line, `>` or not; 0 on a task line outside a quote.
    pub depth: usize,
    /// The quote part's end, past its last `>` and the one space or tab after it; where a task
    /// part starts, and `range.end` with no task.
    pub quote_end: ByteOffset,
    /// Set only on a line that holds all its `>`.
    pub task: Option<Task>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
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
                    let task = Task {
                        checkbox: to_offsets(range.end - 3..range.end),
                        checked: *checked,
                    };
                    match task_gutter(text, marker, task, depth) {
                        Some(gutter) => {
                            done = checked.then_some(index);
                            self.tasks.push(gutter);
                        }
                        None => self.bullets.extend(bullet(text, marker)),
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

    /// The gutters, each task gutter in place of its line's quote gutter, and the bullets' conceals.
    pub(crate) fn gutters(
        self,
        text: &str,
        quotes: &[Range<usize>],
    ) -> (Vec<Gutter>, Vec<Conceal>) {
        let mut quote_gutters = gutters(text, quotes).into_iter().peekable();
        let mut out = Vec::with_capacity(quote_gutters.len() + self.tasks.len());
        for task in self.tasks {
            let line = task.range.start;
            out.extend(std::iter::from_fn(|| {
                quote_gutters.next_if(|gutter| gutter.range.start < line)
            }));
            let replaced = quote_gutters.next_if(|gutter| gutter.range.start == line);
            debug_assert!(
                replaced.is_none_or(|replaced| replaced.quote_end == task.quote_end),
                "a task's quote part differs from its line's"
            );
            out.push(task);
        }
        out.extend(quote_gutters);
        (out, self.bullets)
    }
}

/// A task gets a gutter when its marker leads its line (S3 correction 13): before it, only the
/// line's `>`s and spaces; any other keeps its `•`.
fn task_gutter(text: &str, marker: usize, task: Task, depth: usize) -> Option<Gutter> {
    let line = text[..marker].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[line..].find('\n').map_or(text.len(), |len| line + len);
    let mut gutter = gutter(text, line..end, depth);
    let checkbox = task.checkbox.start.get();
    let leads = gutter.quotes.len() == gutter.depth
        && text
            .get(gutter.range.end.get()..marker)
            .is_some_and(|gap| gap.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
        && !text[marker..checkbox].contains('\n');
    leads.then(|| {
        let end = task.checkbox.end.get();
        let blank = matches!(text.as_bytes().get(end), Some(b' ' | b'\t'));
        gutter.range.end = ByteOffset::new(end + usize::from(blank));
        gutter.task = Some(task);
        gutter
    })
}

/// An indented top-level item's range starts at its indentation.
fn marker_start(text: &str, item: usize) -> usize {
    item + text.as_bytes()[item..]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
        .count()
}

/// The element is the bullet byte alone, so a caret at the item's content does not reveal it. An
/// ordered item's marker starts with a digit (`CommonMark` 5.2), so it keeps its number.
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

/// One gutter per line inside a leading quote; `quotes` holds their ranges, without a trailing
/// `\n`, in start order.
fn gutters(text: &str, quotes: &[Range<usize>]) -> Vec<Gutter> {
    let mut gutters = Vec::new();
    // The end of each quote open on this line.
    let mut open: Vec<usize> = Vec::new();
    let mut rest = quotes;
    let mut start = 0;
    loop {
        if open.is_empty() {
            let Some(next) = rest.first() else {
                break;
            };
            start = text[..next.start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
        }
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |len| start + len);
        let entering = rest.iter().take_while(|quote| quote.start <= end).count();
        open.extend(rest[..entering].iter().map(|quote| quote.end));
        rest = &rest[entering..];
        gutters.push(gutter(text, start..end, open.len()));
        open.retain(|&quote_end| end < quote_end);
        start = end + 1;
    }
    gutters
}

fn gutter_at(start: usize, depth: usize) -> Gutter {
    Gutter {
        range: to_offsets(start..start),
        quotes: Vec::new(),
        depth,
        quote_end: ByteOffset::new(start),
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
    gutter.quote_end = gutter.range.end;
    gutter
}

/// One conceal per `>`, all with the element from the gutter's start through its last `>`, so a
/// line's `>`s reveal together and a caret after the last one's space reveals none. A task's
/// conceal follows, its element and marker the task part, so the two parts reveal apart (D2).
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
        let task = gutter.task.as_ref().map(|task| {
            let part = gutter.quote_end..task.checkbox.end;
            Conceal {
                element: part.clone(),
                marker: part,
                replacement: "",
            }
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
        .map(|gutter| gutter.range.start.get()..gutter.quote_end.get())
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
