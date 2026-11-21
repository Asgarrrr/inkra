//! `⌘L`: the splices that cycle the task state of the selection's lines (task boxes spec, D6).

use std::ops::Range;
use std::vec;

use inkra_text::{ByteOffset, Hunk};

use crate::items::{column_after, marker_end};
use crate::parse::line_start_prefix;
use crate::spans::{line_ranges, line_start};
use crate::structure::{Structure, innermost_item};
use crate::{Markdown, parse};

/// Parses of a spliced copy per `⌘L`: the first candidates, the second ones, then the batch
/// without the lines no candidate makes a task (D6, amended for T4b).
const ROUNDS: usize = 3;

/// One cycled line: its start, and the splices still to try, in order.
struct Cycled {
    start: usize,
    /// Its one candidate is its task's flip, which lands as it stands (D6).
    flips: bool,
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
/// quote still opens at its byte, so no markup turns to text. None for a line `structure` skips,
/// nor for a blank line when the selection spans more than one.
#[must_use]
pub fn cycle_tasks(text: &str, markdown: &Markdown, selection: Range<ByteOffset>) -> Vec<Hunk> {
    let lines = cycled_lines(text, &markdown.structure.skipped, &selection);
    let Some(first) = lines.first() else {
        return Vec::new();
    };
    let at = |line: &Range<usize>| markdown.task_at(ByteOffset::new(line.start));
    let checked = at(first).is_some_and(|task| !task.checked);
    let mut cycled: Vec<Cycled> = lines
        .into_iter()
        .map(|line| Cycled {
            start: line.start,
            flips: at(&line).is_some(),
            candidates: candidates(text, markdown, line, checked).into_iter(),
        })
        .collect();
    for _ in 0..ROUNDS {
        cycled.retain(|line| line.splice().is_some());
        if cycled.is_empty() {
            break;
        }
        let landed = landed(text, &markdown.structure, &cycled, checked);
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
/// before the selection's end (H). Skipped lines go, a range's end included, and blank lines when
/// there are several.
fn cycled_lines(
    text: &str,
    skipped: &[Range<ByteOffset>],
    selection: &Range<ByteOffset>,
) -> Vec<Range<usize>> {
    let first = line_start(text, selection.start.get());
    let lines: Vec<Range<usize>> = line_ranges(text, first)
        .take_while(|line| line.start == first || line.start < selection.end.get())
        .collect();
    let several = lines.len() > 1;
    let mut skipped = skipped.iter().peekable();
    lines
        .into_iter()
        .filter(|line| {
            !(several
                && text[line.clone()]
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t')))
        })
        .filter(|line| {
            while skipped
                .next_if(|range| range.end.get() < line.start)
                .is_some()
            {}
            skipped
                .peek()
                .is_none_or(|range| range.start.get() > line.start)
        })
        .collect()
}

/// The splices that could put `line` in the state `checked`, in order: a task's flip, or none when
/// it is in that state already; a line where an item opens gains a checkbox at its innermost
/// item's content start; any other line at the fallback's, else `- ` and a checkbox after its
/// leading `[ \t>]*`.
fn candidates(text: &str, markdown: &Markdown, line: Range<usize>, checked: bool) -> Vec<Hunk> {
    if let Some(task) = markdown.task_at(ByteOffset::new(line.start)) {
        return (task.checked != checked)
            .then(|| task.flip())
            .into_iter()
            .collect();
    }
    let checkbox = if checked { "[x] " } else { "[ ] " };
    let items = &markdown.structure.items;
    let innermost = items[..items.partition_point(|item| item.marker.get() < line.end)]
        .last()
        .map(|item| item.marker.get())
        .filter(|&marker| marker >= line.start);
    let splices: Vec<(usize, &str)> = if let Some(marker) = innermost {
        vec![checkbox_at(text, line.start, marker)]
    } else {
        let lead = line_start_prefix(text, &line);
        fallback(text, &line, lead)
            .into_iter()
            .chain([(line.start + lead, "- ")])
            .collect()
    };
    splices
        .into_iter()
        .map(|(at, before)| {
            let at = ByteOffset::new(at);
            Hunk {
                range: at..at,
                text: format!("{before}{checkbox}"),
            }
        })
        .collect()
}

/// Where a checkbox goes after the item marker at `marker`, on the line starting at `line`, and
/// what goes before it: the content start (D6). Its blanks count in columns from the line's start;
/// one to four go before it, of five or more only one, and an empty item gains a space of its own.
fn checkbox_at(text: &str, line: usize, marker: usize) -> (usize, &'static str) {
    let bytes = text.as_bytes();
    let end = marker_end(text, marker);
    let blanks = bytes[end..]
        .iter()
        .take(5)
        .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
        .count();
    let after = column_after(0, &bytes[line..end]);
    match column_after(after, &bytes[end..end + blanks]) - after {
        0 => (end, " "),
        1..=4 => (end + blanks, ""),
        _ => (end + 1, ""),
    }
}

/// Where the innermost item pulldown-cmark reads on `line` alone, past its prefix of `lead` bytes,
/// puts a checkbox. The pass starts its tab stops at its text, so the text follows `c mod 4` spaces,
/// `c` the prefix's columns: every tab stops where it does in the line, and the pad opens no code
/// (D6).
fn fallback(text: &str, line: &Range<usize>, lead: usize) -> Option<(usize, &'static str)> {
    #[cfg(test)]
    counted(&FALLBACKS);
    let from = line.start + lead;
    let pad = column_after(0, &text.as_bytes()[line.start..from]) % 4;
    let piece = format!("{:pad$}{}\n", "", &text[from..line.end]);
    let (at, before) = checkbox_at(&piece, 0, innermost_item(&piece)?);
    Some((from + at - pad, before))
}

/// Whether each line's current splice, applied with all the others to one copy, leaves the line
/// a task in the state `checked` as one parse of the copy reads it. An item or quote of `before`,
/// the structure of `text`, that no longer opens at its byte in the copy fails the nearest cycled
/// line at or above it: a splice reaches its own line and those below. Every cycled line has a
/// splice. A round of flips only lands as it stands, and parses nothing (D6).
fn landed(text: &str, before: &Structure, cycled: &[Cycled], checked: bool) -> Vec<bool> {
    if cycled.iter().all(|line| line.flips) {
        return vec![true; cycled.len()];
    }
    let hunks: Vec<&Hunk> = cycled.iter().filter_map(Cycled::splice).collect();
    let mut spliced = String::with_capacity(text.len());
    let mut copied = 0;
    for hunk in &hunks {
        spliced.push_str(&text[copied..hunk.range.start.get()]);
        spliced.push_str(&hunk.text);
        copied = hunk.range.end.get();
    }
    spliced.push_str(&text[copied..]);
    #[cfg(test)]
    counted(&PARSES);
    let parsed = parse(&spliced);
    // A splice lies inside its line, so a line moves by the growth of the splices before it.
    let mut shift = 0;
    let mut landed: Vec<bool> = cycled
        .iter()
        .zip(&hunks)
        .map(|(line, hunk)| {
            let start = ByteOffset::new(line.start + shift);
            let landed = parsed
                .task_at(start)
                .is_some_and(|task| task.checked == checked);
            shift += hunk.text.len() - (hunk.range.end - hunk.range.start);
            landed
        })
        .collect();
    let after = &parsed.structure;
    let items =
        moved(before.items.iter().map(|item| item.marker.get()), &hunks).filter(|&(_, to)| {
            after
                .items
                .binary_search_by_key(&ByteOffset::new(to), |item| item.marker)
                .is_err()
        });
    let quotes = before.quotes.iter().map(|quote| quote.get());
    let quotes = moved(quotes, &hunks)
        .filter(|&(_, to)| after.quotes.binary_search(&ByteOffset::new(to)).is_err());
    for (at, _) in items.chain(quotes) {
        let blamed = cycled.partition_point(|line| line.start <= at);
        landed[blamed.saturating_sub(1)] = false;
    }
    landed
}

/// Each byte of `at`, sorted, and where it lands in the copy `hunks` make: after an insertion at
/// it.
fn moved<'a>(
    at: impl Iterator<Item = usize> + 'a,
    hunks: &'a [&Hunk],
) -> impl Iterator<Item = (usize, usize)> + 'a {
    let mut hunks = hunks.iter().peekable();
    let mut shift = 0;
    at.map(move |at| {
        while let Some(hunk) = hunks.next_if(|hunk| hunk.range.end.get() <= at) {
            shift += hunk.text.len() - (hunk.range.end - hunk.range.start);
        }
        (at, at + shift)
    })
}

#[cfg(test)]
fn counted(passes: &'static std::thread::LocalKey<std::cell::Cell<usize>>) {
    passes.set(passes.get() + 1);
}

#[cfg(test)]
thread_local! {
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod batch_tests;

#[cfg(test)]
mod fallback_tests;
