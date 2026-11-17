//! `⌘L`: the splices that cycle a line's task state (task boxes spec, D6).

use std::ops::Range;

use inkra_text::{ByteOffset, Hunk};
use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;

/// The splices that cycle the task state of the lines `lines` touches, which lie on one line: an
/// unchecked task is checked, a checked one cleared, any other line becomes an unchecked task.
/// None for a line inside a code block, the front matter or a table.
#[must_use]
pub fn cycle_tasks(text: &str, lines: Range<ByteOffset>) -> Vec<Hunk> {
    let start = line_start(text, lines.start.get());
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |len| start + len);
    debug_assert!(lines.end.get() <= end, "a range across lines");
    if skipped(text).any(|block| block.contains(&start)) {
        return Vec::new();
    }
    let marker = Marker::read(&text[start..end]);
    vec![marker.splice(start, marker.next())]
}

/// A line's marker as D6 reads it: leading spaces, tabs and `>`s, a bullet, a checkbox.
enum Marker {
    /// The checkbox's middle byte, from the line's start.
    Task { middle: usize, checked: bool },
    /// Past the bullet and its one space or tab: where a checkbox goes.
    Bullet(usize),
    /// Past the leading spaces, tabs and `>`s: where a bullet and a checkbox go.
    Plain(usize),
}

impl Marker {
    fn read(line: &str) -> Self {
        let bytes = line.as_bytes();
        let lead = leading(bytes, |byte| matches!(byte, b' ' | b'\t' | b'>'));
        let Some(bullet) = bullet(&bytes[lead..]) else {
            return Self::Plain(lead);
        };
        let after = lead + bullet;
        let checkbox = after + leading(&bytes[after..], |byte| matches!(byte, b' ' | b'\t'));
        match bytes.get(checkbox..checkbox + 3) {
            Some([b'[', middle @ (b' ' | b'x' | b'X'), b']']) => Self::Task {
                middle: checkbox + 1,
                checked: *middle != b' ',
            },
            _ => Self::Bullet(after + 1),
        }
    }

    /// Checked after an unchecked task, unchecked after anything else.
    fn next(&self) -> bool {
        matches!(self, Self::Task { checked: false, .. })
    }

    /// The splice that leaves the line, starting at `line`, a task in state `checked`.
    fn splice(&self, line: usize, checked: bool) -> Hunk {
        let mark = if checked { 'x' } else { ' ' };
        let (at, removed, text) = match *self {
            Self::Task { middle, .. } => (middle, 1, mark.to_string()),
            Self::Bullet(at) => (at, 0, format!("[{mark}] ")),
            Self::Plain(at) => (at, 0, format!("- [{mark}] ")),
        };
        let at = ByteOffset::new(line + at);
        Hunk {
            range: at..at + removed,
            text,
        }
    }
}

/// The bullet's length, `-`, `*`, `+` or one to nine digits and `.` or `)` (`CommonMark` 5.2),
/// when a space or tab follows it.
fn bullet(bytes: &[u8]) -> Option<usize> {
    let digits = leading(&bytes[..bytes.len().min(9)], |byte| byte.is_ascii_digit());
    let len = match (digits, bytes.get(digits)) {
        (0, Some(b'-' | b'*' | b'+')) => 1,
        (1.., Some(b'.' | b')')) => digits + 1,
        _ => return None,
    };
    matches!(bytes.get(len), Some(b' ' | b'\t')).then_some(len)
}

fn leading(bytes: &[u8], is: impl Fn(u8) -> bool) -> usize {
    bytes.iter().take_while(|&&byte| is(byte)).count()
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Each code block, front matter and table, from its first line's start: a fenced block in a
/// list item starts at its fence, past the line's start.
fn skipped(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    Parser::new_ext(text, options())
        .into_offset_iter()
        .filter(|(event, _)| {
            matches!(
                event,
                Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_))
            )
        })
        .map(|(_, range)| line_start(text, range.start)..range.end)
}

#[cfg(test)]
mod tests;
