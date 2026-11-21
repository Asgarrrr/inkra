//! The oracle: each item and leading quote as a pulldown-cmark pass of its own reads them.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag};

use crate::cmark::options;
use crate::spans::line_start;

/// An item as the oracle reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Read {
    pub(super) marker: usize,
    pub(super) end: usize,
    /// Its item ancestors.
    pub(super) depth: usize,
    /// Its list's index, in document order.
    pub(super) list: usize,
    /// Whether its ancestors are leading quotes, gutter items and their lists.
    pub(super) gutter: bool,
}

/// Each item, and each leading quote's range, by a pulldown-cmark pass of its own.
pub(super) fn oracle(text: &str) -> (Vec<Read>, Vec<Range<usize>>) {
    let text = format!("{text}\n");
    // Each open element: its tag kind, whether it is in the gutter, and its list's index.
    let mut open: Vec<(u8, bool, usize, bool)> = Vec::new();
    let (mut items, mut quotes, mut lists) = (Vec::new(), Vec::new(), 0);
    for (event, range) in Parser::new_ext(&text, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                let parent = open.last().copied();
                let leading = matches!(tag, Tag::BlockQuote(_)) && parent.is_none_or(|p| p.3);
                let listed = matches!(tag, Tag::List(_) | Tag::Item) && parent.is_none_or(|p| p.1);
                let list = match tag {
                    Tag::List(_) => {
                        lists += 1;
                        lists - 1
                    }
                    _ => parent.map_or(0, |p| p.2),
                };
                if matches!(tag, Tag::Item) {
                    let skip = text.as_bytes()[range.start..]
                        .iter()
                        .take_while(|b| b" \t\n>".contains(b))
                        .count();
                    items.push(Read {
                        marker: range.start + skip,
                        end: range.end,
                        depth: open.iter().filter(|element| element.0 == 1).count(),
                        list,
                        gutter: listed,
                    });
                }
                if leading {
                    quotes.push(range.clone());
                }
                let kind = u8::from(matches!(tag, Tag::Item));
                open.push((kind, leading || listed, list, leading));
            }
            Event::End(_) => {
                open.pop();
            }
            _ => {}
        }
    }
    (items, quotes)
}

/// Each item's depth and list, the lists renumbered by first appearance.
pub(super) fn shape(items: &[Read]) -> Vec<(usize, usize)> {
    let mut seen: Vec<usize> = Vec::new();
    items
        .iter()
        .map(|item| {
            let list = seen
                .iter()
                .position(|&list| list == item.list)
                .unwrap_or_else(|| {
                    seen.push(item.list);
                    seen.len() - 1
                });
            (item.depth, list)
        })
        .collect()
}

/// The leading quotes and the gutter items holding the line starting at `line`, or opening on it.
pub(super) fn containers(text: &str, line: usize) -> (usize, usize) {
    let (items, quotes) = oracle(text);
    let inside = quotes
        .iter()
        .filter(|quote| line_start(text, quote.start) <= line && line < quote.end)
        .count();
    let held = items
        .iter()
        .filter(|item| item.gutter && line_start(text, item.marker) <= line && line < item.end)
        .count();
    (inside, held)
}
