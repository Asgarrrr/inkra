//! `⌘L`: the splices that cycle a line's task state (task boxes spec, D6).

use std::ops::Range;

use inkra_text::{ByteOffset, Hunk};
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::parse::line_start_prefix;
use crate::spans::line_start;
use crate::{Markdown, Task, parse};

/// The splices that cycle the task state of the lines `lines` touches, which lie on one line, as
/// `markdown`, the parse of `text`, reads it: a task's checkbox flips, and any other line gains
/// the first marker the reparse makes an unchecked task, else nothing. None for a line inside a
/// code block, the front matter, a table or an HTML block.
#[must_use]
pub fn cycle_tasks(text: &str, markdown: &Markdown, lines: Range<ByteOffset>) -> Vec<Hunk> {
    let start = line_start(text, lines.start.get());
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |len| start + len);
    debug_assert!(lines.end.get() <= end, "a range across lines");
    if let Some(task) = task_at(markdown, start) {
        return vec![task.flip()];
    }
    if skipped(text).any(|block| block.contains(&start)) {
        return Vec::new();
    }
    let lead = line_start_prefix(text, &(start..end));
    bullet(&text.as_bytes()[start + lead..end])
        .map(|(at, marker)| (lead + at, marker))
        .into_iter()
        .chain([(lead, "- [ ] ")])
        .map(|(at, marker)| {
            let at = ByteOffset::new(start + at);
            Hunk {
                range: at..at,
                text: marker.to_owned(),
            }
        })
        .find(|hunk| {
            let mut spliced = text.to_owned();
            spliced.insert_str(hunk.range.start.get(), &hunk.text);
            task_at(&parse(&spliced), start).is_some_and(|task| !task.checked)
        })
        .into_iter()
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
/// `)` (`CommonMark` 5.2): past its one space or tab, or with a space of its own on an empty item.
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
        Some(b' ' | b'\t') => Some((len + 1, "[ ] ")),
        None => Some((len, " [ ] ")),
        Some(_) => None,
    }
}

/// Each code block, front matter, table and HTML block, from its first line's start: a fenced
/// block in a list item starts at its fence, past the line's start.
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
