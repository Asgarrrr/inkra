use std::borrow::Cow;
use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Parser, Tag};

use crate::parse::{Unrepeated, backtick_runs, options};

/// Leads a mid-line piece: whitespace to the emphasis rules, yet no block marker can follow it.
const INSIDE: &str = "\u{a0}";

/// Bytes the plain form drops: leading block markers and emphasis, strong and code-span delimiters.
/// `line` holds no `\n`; link, strikethrough and code-block syntax stays.
#[must_use]
pub fn markup(line: &str, at_line_start: bool) -> Vec<Range<ByteOffset>> {
    let (text, shift) = if at_line_start {
        (Cow::Borrowed(line), 0)
    } else {
        (Cow::Owned(format!("{INSIDE}{line}")), INSIDE.len())
    };
    let mut out = Out::default();
    // Each open element's end, and whether the bytes between its children drop.
    let mut open: Vec<(usize, bool)> = Vec::new();
    for (event, range) in Unrepeated::new(Parser::new_ext(&text, options()).into_offset_iter()) {
        let current = open.last().copied();
        match event {
            Event::Start(tag) => {
                out.gap(range.start, current);
                open.push((range.end, drops(&tag)));
            }
            Event::End(_) => {
                out.gap(range.end, current);
                open.pop();
            }
            Event::Text(_) | Event::Html(_) | Event::InlineHtml(_) => {
                out.gap(range.start, current);
                out.cursor = out.cursor.max(range.end);
            }
            Event::Code(_) => {
                out.gap(range.start, current);
                let (start, end) = backtick_runs(&text, &range);
                out.push(range.start..start);
                out.push(end..range.end);
                out.cursor = range.end;
            }
            Event::Rule => {
                out.gap(range.start, current);
                out.push(range.clone());
                out.cursor = range.end;
            }
            // Task boxes are left to the next gap, which their item drops.
            _ => {}
        }
    }
    out.ranges
        .into_iter()
        .map(|range| ByteOffset(range.start - shift)..ByteOffset(range.end - shift))
        .collect()
}

fn drops(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::Item
            | Tag::Emphasis
            | Tag::Strong
    )
}

#[derive(Default)]
struct Out {
    ranges: Vec<Range<usize>>,
    /// Every byte before this one is dropped or known to stay.
    cursor: usize,
}

impl Out {
    /// Bytes from the cursor to `end` no leaf covers: `current`'s syntax, which drops them or not.
    fn gap(&mut self, end: usize, current: Option<(usize, bool)>) {
        let end = current.map_or(end, |(close, _)| end.min(close));
        if current.is_some_and(|(_, drops)| drops) {
            self.push(self.cursor..end);
        }
        self.cursor = self.cursor.max(end);
    }

    fn push(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        match self.ranges.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => self.ranges.push(range),
        }
    }
}

#[cfg(test)]
mod tests;
