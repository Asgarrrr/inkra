//! `⌘L`: the splices that cycle the task state of the selection's lines (task boxes spec, D6).

use std::ops::Range;
use std::vec;

use inkra_text::{ByteOffset, Hunk};
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::parse::line_start_prefix;
use crate::spans::{line_ranges, line_start};
use crate::{Markdown, Task, parse};

/// Parses of a spliced copy per `⌘L`: the first candidates, the second ones, then the batch
/// without the lines no candidate makes a task (D6, amended for T4b).
const ROUNDS: usize = 3;

/// One cycled line: its start and the splices still to try, in order.
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
/// batch, applied at once, reparses each changed line as a task in that state. None for a line
/// inside a code block, the front matter, a table or an HTML block, nor for a blank line when
/// the selection spans more than one.
#[must_use]
pub fn cycle_tasks(text: &str, markdown: &Markdown, selection: Range<ByteOffset>) -> Vec<Hunk> {
    let lines = cycled_lines(text, &selection);
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
        let landed = landed(text, &cycled, checked);
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
/// before the selection's end (H). Skipped blocks go, and blank lines when there are several.
fn cycled_lines(text: &str, selection: &Range<ByteOffset>) -> Vec<Range<usize>> {
    let first = line_start(text, selection.start.get());
    let lines: Vec<Range<usize>> = line_ranges(text, first)
        .take_while(|line| line.start == first || line.start < selection.end.get())
        .collect();
    let several = lines.len() > 1;
    let mut blocks = skipped(text).peekable();
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
/// it is in that state already; any other line gains a checkbox after its bullet, else `- ` and a
/// checkbox after its leading `[ \t>]*`.
fn candidates(text: &str, markdown: &Markdown, line: Range<usize>, checked: bool) -> Vec<Hunk> {
    if let Some(task) = task_at(markdown, line.start) {
        return (task.checked != checked)
            .then(|| task.flip())
            .into_iter()
            .collect();
    }
    let checkbox = if checked { "[x] " } else { "[ ] " };
    let lead = line_start_prefix(text, &line);
    bullet(&text.as_bytes()[line.start + lead..line.end])
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
/// a task in the state `checked`, as one parse of the copy reads it.
fn landed(text: &str, cycled: &[Cycled], checked: bool) -> Vec<bool> {
    let hunks: Vec<(usize, &Hunk)> = cycled
        .iter()
        .filter_map(|line| Some((line.start, line.splice()?)))
        .collect();
    let mut spliced = String::with_capacity(text.len());
    let mut copied = 0;
    for (_, hunk) in &hunks {
        spliced.push_str(&text[copied..hunk.range.start.get()]);
        spliced.push_str(&hunk.text);
        copied = hunk.range.end.get();
    }
    spliced.push_str(&text[copied..]);
    let parsed = parse(&spliced);
    // A splice lies inside its line, so a line moves by the growth of the splices before it.
    let mut shift = 0;
    hunks
        .into_iter()
        .map(|(start, hunk)| {
            let landed =
                task_at(&parsed, start + shift).is_some_and(|task| task.checked == checked);
            shift += hunk.text.len() - (hunk.range.end - hunk.range.start);
            landed
        })
        .collect()
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

/// Where a checkbox goes after a leading bullet, `-`, `*`, `+` or one to nine digits and `.` or
/// `)` (`CommonMark` 5.2), and what goes before it: past its one space or tab, or a space of its
/// own on an empty item.
fn bullet(bytes: &[u8]) -> Option<(usize, &'static str)> {
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
    match bytes.get(len) {
        Some(b' ' | b'\t') => Some((len + 1, "")),
        None => Some((len, " ")),
        Some(_) => None,
    }
}

/// Each code block, front matter, table and HTML block, from its first line's start, in order: a
/// fenced block in a list item starts at its fence, past the line's start. None holds another.
fn skipped(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    Parser::new_ext(text, options())
        .into_offset_iter()
        .filter(|(event, _)| {
            matches!(
                event,
                Event::Start(
                    Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_) | Tag::HtmlBlock
                )
            )
        })
        .map(|(_, range)| line_start(text, range.start)..range.end)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod batch_tests;
