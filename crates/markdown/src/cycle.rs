//! `⌘L`: the splices that cycle the task state of the selection's lines (task boxes spec, D6).

use std::borrow::Cow;
use std::ops::Range;
use std::vec;

use inkra_text::{ByteOffset, Hunk};
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::gutters::marker_start;
use crate::parse::line_start_prefix;
use crate::spans::{line_ranges, line_start};
use crate::{Markdown, Task, parse};

/// Parses of a spliced copy per `⌘L`: the first candidates, the second ones, then the batch
/// without the lines no candidate makes a task (D6, amended for T4b).
const ROUNDS: usize = 3;

/// One cycled line: its start, and the splices still to try, in order.
struct Cycled {
    start: usize,
    candidates: vec::IntoIter<Hunk>,
}

impl Cycled {
    fn splice(&self) -> Option<&Hunk> {
        self.candidates.as_slice().first()
    }
}

/// The splices that cycle the task state of every line `selection` touches, as `markdown`, the
/// parse of `text`, reads it: each takes the state after the first cycled line's, and the whole
/// batch, applied at once, reparses each changed line as a task in that state, and every item and
/// quote still opens at its byte, so no markup turns to text. None for a line inside
/// a code block, the front matter, a table or an HTML block, nor for a line holding a task with
/// no box, nor for a blank line when the selection spans more than one.
#[must_use]
pub fn cycle_tasks(text: &str, markdown: &Markdown, selection: Range<ByteOffset>) -> Vec<Hunk> {
    let scan = Scan::new(text, markdown);
    let lines = cycled_lines(text, &scan.skipped, &selection);
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    let checked = task_at(markdown, first.start).is_some_and(|task| !task.checked);
    let mut cycled: Vec<Cycled> = lines
        .into_iter()
        .map(|line| Cycled {
            start: line.start,
            candidates: candidates(text, markdown, line, checked).into_iter(),
        })
        .collect();
    for _ in 0..ROUNDS {
        cycled.retain(|line| line.splice().is_some());
        if cycled.is_empty() {
            break;
        }
        let landed = landed(text, &scan, &cycled, checked);
        if landed.iter().all(|&landed| landed) {
            return cycled
                .into_iter()
                .filter_map(|mut line| line.candidates.next())
                .collect();
        }
        for (line, landed) in cycled.iter_mut().zip(landed) {
            if !landed {
                line.candidates.next();
            }
        }
    }
    Vec::new()
}

/// The lines `selection` touches, each without its newline: the first, then every line starting
/// before the selection's end (H). Skipped lines go, and blank lines when there are several.
fn cycled_lines(
    text: &str,
    skipped: &[Range<usize>],
    selection: &Range<ByteOffset>,
) -> Vec<Range<usize>> {
    let first = line_start(text, selection.start.get());
    let lines: Vec<Range<usize>> = line_ranges(text, first)
        .take_while(|line| line.start == first || line.start < selection.end.get())
        .collect();
    let several = lines.len() > 1;
    let mut blocks = skipped.iter().peekable();
    lines
        .into_iter()
        .filter(|line| {
            !(several
                && text[line.clone()]
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t')))
        })
        .filter(|line| {
            while blocks.next_if(|block| block.end <= line.start).is_some() {}
            blocks.peek().is_none_or(|block| block.start > line.start)
        })
        .collect()
}

/// The splices that could put `line` in the state `checked`, in order: a task's flip, or none when
/// it is in that state already; any other line gains a checkbox after its innermost bullet, else
/// `- ` and a checkbox after its leading `[ \t>]*`.
fn candidates(text: &str, markdown: &Markdown, line: Range<usize>, checked: bool) -> Vec<Hunk> {
    if let Some(task) = task_at(markdown, line.start) {
        return (task.checked != checked)
            .then(|| task.flip())
            .into_iter()
            .collect();
    }
    let checkbox = if checked { "[x] " } else { "[ ] " };
    let lead = line_start_prefix(text, &line);
    innermost(&text.as_bytes()[line.start + lead..line.end])
        .map(|(at, space)| (lead + at, space))
        .into_iter()
        .chain([(lead, "- ")])
        .map(|(at, before)| {
            let at = ByteOffset::new(line.start + at);
            Hunk {
                range: at..at,
                text: format!("{before}{checkbox}"),
            }
        })
        .collect()
}

/// Whether each line's current splice, applied with all the others to one copy, leaves the line
/// a task in the state `checked` as one parse of the copy reads it. An item or quote of `before`,
/// the pass of `text`, that no longer opens at its byte in the copy fails the nearest cycled line
/// at or above it: a splice reaches its own line and those below. Every cycled line has a splice.
fn landed(text: &str, before: &Scan, cycled: &[Cycled], checked: bool) -> Vec<bool> {
    let hunks: Vec<&Hunk> = cycled.iter().filter_map(Cycled::splice).collect();
    let mut spliced = String::with_capacity(text.len());
    let mut copied = 0;
    for hunk in &hunks {
        spliced.push_str(&text[copied..hunk.range.start.get()]);
        spliced.push_str(&hunk.text);
        copied = hunk.range.end.get();
    }
    spliced.push_str(&text[copied..]);
    let parsed = parse(&spliced);
    // A splice lies inside its line, so a line moves by the growth of the splices before it.
    let mut shift = 0;
    let mut landed: Vec<bool> = cycled
        .iter()
        .zip(&hunks)
        .map(|(line, hunk)| {
            let landed =
                task_at(&parsed, line.start + shift).is_some_and(|task| task.checked == checked);
            shift += hunk.text.len() - (hunk.range.end - hunk.range.start);
            landed
        })
        .collect();
    let after = Openers::new(&spliced);
    let items = moved(&before.openers.items, &hunks).filter(|(_, to)| !after.has_item(*to));
    let quotes = moved(&before.openers.quotes, &hunks).filter(|(_, to)| !after.has_quote(*to));
    for (at, _) in items.chain(quotes) {
        let blamed = cycled.partition_point(|line| line.start <= at);
        landed[blamed.saturating_sub(1)] = false;
    }
    landed
}

/// Each byte of `at`, sorted, and where it lands in the copy `hunks` make: after an insertion at
/// it.
fn moved<'a>(at: &'a [usize], hunks: &'a [&Hunk]) -> impl Iterator<Item = (usize, usize)> + 'a {
    let mut hunks = hunks.iter().peekable();
    let mut shift = 0;
    at.iter().map(move |&at| {
        while let Some(hunk) = hunks.next_if(|hunk| hunk.range.end.get() <= at) {
            shift += hunk.text.len() - (hunk.range.end - hunk.range.start);
        }
        (at, at + shift)
    })
}

/// The task whose gutter starts at the line start `start`.
fn task_at(markdown: &Markdown, start: usize) -> Option<&Task> {
    let gutters = &markdown.gutters;
    let at = gutters.partition_point(|gutter| gutter.range.start.get() < start);
    gutters
        .get(at)
        .filter(|gutter| gutter.range.start.get() == start)
        .and_then(|gutter| gutter.task.as_ref())
}

/// A leading list marker, `-`, `*`, `+` or one to nine digits and `.` or `)` (`CommonMark` 5.2):
/// its length, and the blanks after it, up to five; `None` when a byte other than a blank follows.
fn marker(bytes: &[u8]) -> Option<(usize, usize)> {
    let digits = bytes
        .iter()
        .take(9)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let len = match (digits, bytes.get(digits)) {
        (0, Some(b'-' | b'*' | b'+')) => 1,
        (1.., Some(b'.' | b')')) => digits + 1,
        _ => return None,
    };
    let blanks = bytes[len..]
        .iter()
        .take(5)
        .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
        .count();
    (blanks > 0 || len == bytes.len()).then_some((len, blanks))
}

/// Where a checkbox goes after the innermost of the leading markers, and what goes before it;
/// `None` with no marker. A marker opens another past one to four blanks (`CommonMark` 5.2); the
/// checkbox goes there too, at the content's column, past one blank of five or more, or after a
/// space of its own on an empty item.
fn innermost(bytes: &[u8]) -> Option<(usize, &'static str)> {
    let first = (0, marker(bytes)?);
    let (at, (len, blanks)) = std::iter::successors(Some(first), |&(at, (len, blanks))| {
        let next = at + len + blanks;
        marker(&bytes[next..])
            .filter(|_| (1..=4).contains(&blanks))
            .map(|marker| (next, marker))
    })
    .take(bytes.len())
    .last()?;
    Some(match blanks {
        0 => (at + len, " "),
        1..=4 => (at + len + blanks, ""),
        _ => (at + len + 1, ""),
    })
}

/// What one pulldown-cmark pass reads of a text, as `parse` reads it (D1).
struct Scan {
    /// Each code block, front matter, table and HTML block from its first line's start, and each
    /// line holding a task with no box (D10), in document order: a fenced block in a list item
    /// starts at its fence, past the line's start. None holds another.
    skipped: Vec<Range<usize>>,
    openers: Openers,
}

impl Scan {
    fn new(text: &str, markdown: &Markdown) -> Self {
        let text = terminated(text);
        let mut scan = Self {
            skipped: Vec::new(),
            openers: Openers::default(),
        };
        for (event, range) in Parser::new_ext(&text, options()).into_offset_iter() {
            let start = line_start(&text, range.start);
            match event {
                Event::Start(
                    Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_) | Tag::HtmlBlock,
                ) => scan.skipped.push(start..range.end),
                Event::TaskListMarker(_) if task_at(markdown, start).is_none() => {
                    scan.skipped.push(start..range.end);
                }
                event => scan.openers.push(&text, &event, range.start),
            }
        }
        debug_assert!(scan.openers.items.is_sorted() && scan.openers.quotes.is_sorted());
        scan
    }
}

/// The bytes where list items and block quotes open in a text, as `parse` reads it.
#[derive(Default)]
struct Openers {
    /// Each item's marker, in document order, so sorted: a nested opener starts past its parent.
    items: Vec<usize>,
    /// Each quote's `>`, where pulldown-cmark starts its range, sorted the same way.
    quotes: Vec<usize>,
}

impl Openers {
    fn new(text: &str) -> Self {
        let text = terminated(text);
        let mut openers = Self::default();
        for (event, range) in Parser::new_ext(&text, options()).into_offset_iter() {
            openers.push(&text, &event, range.start);
        }
        debug_assert!(openers.items.is_sorted() && openers.quotes.is_sorted());
        openers
    }

    fn has_item(&self, at: usize) -> bool {
        self.items.binary_search(&at).is_ok()
    }

    fn has_quote(&self, at: usize) -> bool {
        self.quotes.binary_search(&at).is_ok()
    }

    /// Records the opener `event` starts at `start`, if any.
    fn push(&mut self, text: &str, event: &Event, start: usize) {
        match event {
            Event::Start(Tag::Item) => self.items.push(marker_start(text, start)),
            Event::Start(Tag::BlockQuote(_)) => self.quotes.push(start),
            _ => {}
        }
    }
}

/// `text` as `parse` reads it, ending with a newline (D1).
fn terminated(text: &str) -> Cow<'_, str> {
    if text.ends_with('\n') {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(format!("{text}\n"))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod batch_tests;
