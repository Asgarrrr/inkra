//! The oracle: each item and leading quote as a pulldown-cmark pass of its own reads them.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag};

use crate::ListEdit;
use crate::cmark::options;
use crate::spans::{line_end, line_start};
use crate::testing::landing;

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

/// A list no item of the old text is in.
const FRESH: usize = usize::MAX;

/// The item a command acts on for the line starting at `line`: the innermost gutter item opening
/// on it, else the innermost holding it.
pub(super) fn caret_item(text: &str, items: &[Read], line: usize) -> Option<usize> {
    let gutter = |item: &&Read| item.gutter;
    let opening = items
        .iter()
        .rposition(|item| gutter(&item) && line_start(text, item.marker) == line);
    opening.or_else(|| {
        items.iter().rposition(|item| {
            gutter(&item) && line_start(text, item.marker) < line && line < item.end
        })
    })
}

/// The nearest item above `at` one level up.
pub(super) fn parent(items: &[Read], at: usize) -> Option<usize> {
    let depth = items[at].depth.checked_sub(1)?;
    items[..at].iter().rposition(|item| item.depth == depth)
}

/// The nearest item above `at` in its list.
pub(super) fn previous_sibling(items: &[Read], at: usize) -> Option<usize> {
    items[..at]
        .iter()
        .rposition(|item| item.list == items[at].list)
}

/// `at` and its descendants, as indices.
fn subtree(items: &[Read], at: usize) -> Range<usize> {
    let depth = items[at].depth;
    let count = items[at + 1..]
        .iter()
        .take_while(|item| item.depth > depth)
        .count();
    at..at + 1 + count
}

/// The items `⇥` means for `at`: it and its descendants one level down, it in the list its new
/// parent's last child ends that parent's block with, else in a list of its own (D4). `None` when
/// it has no previous sibling.
pub(super) fn indented(text: &str, items: &[Read], at: usize) -> Option<Vec<Read>> {
    let parent = previous_sibling(items, at)?;
    let joins = last_child(text, items, parent);
    let mut want = items.to_vec();
    for item in &mut want[subtree(items, at)] {
        item.depth += 1;
    }
    want[at].list = joins.map_or(FRESH, |child| items[child].list);
    Some(want)
}

/// Whether only blank lines follow `child`'s block in `parent`'s.
fn ends_block(text: &str, items: &[Read], child: usize, parent: usize) -> bool {
    let end = items[parent].end.min(text.len());
    text[items[child].end.min(end)..end]
        .trim_matches([' ', '\t', '\n', '>'])
        .is_empty()
}

/// The child list `at`'s block ends with, by its last child.
fn last_child(text: &str, items: &[Read], at: usize) -> Option<usize> {
    let depth = items[at].depth + 1;
    subtree(items, at)
        .rev()
        .find(|&child| items[child].depth == depth)
        .filter(|&child| ends_block(text, items, child, at))
}

/// The items `⇧⇥` means for `at`: it and its descendants one level up in its parent's list. The
/// siblings below it, or a list of its parent's right below it, become its children: in the list
/// its children end its block with, else in a list of their own (D4, amendments 3 and 4). `None`
/// when it has no parent.
pub(super) fn outdented(text: &str, items: &[Read], at: usize) -> Option<Vec<Read>> {
    let parent = parent(items, at)?;
    let mut want = items.to_vec();
    let subtree = subtree(items, at);
    for item in &mut want[subtree.clone()] {
        item.depth -= 1;
    }
    want[at].list = items[parent].list;
    let below = subtree.end;
    let next_to = items.get(below).is_none_or(|item| {
        text[items[at].end.min(item.marker)..line_start(text, item.marker)]
            .trim_matches([' ', '\t', '\n', '>'])
            .is_empty()
    });
    let joined = last_child(text, items, at)
        .filter(|_| next_to)
        .map(|child| items[child].list);
    let list = items.get(below).and_then(|item| {
        let line = line_start(text, item.marker);
        let right_below = !text[line_start(text, line.saturating_sub(1))..line]
            .trim_matches([' ', '\t', '\n', '>'])
            .is_empty();
        (item.depth == items[at].depth && (item.list == items[at].list || right_below))
            .then_some(item.list)
    });
    for item in &mut want[below..] {
        if list == Some(item.list) {
            item.list = joined.unwrap_or(if item.list == items[at].list {
                FRESH
            } else {
                item.list
            });
        }
    }
    // Two of its child lists, one right after the other and of one kind, merge, as any two do.
    let depth = items[at].depth;
    let children: Vec<usize> = (at + 1..items.len())
        .take_while(|&next| want[next].depth >= depth)
        .filter(|&next| want[next].depth == depth)
        .collect();
    for pair in children.windows(2) {
        let (above, next) = (pair[0], pair[1]);
        let line = line_start(text, items[next].marker);
        let adjacent = text[items[above].end.min(line)..line]
            .trim_matches([' ', '\t', '\n', '>'])
            .is_empty();
        let (from, to) = (want[next].list, want[above].list);
        if from != to
            && adjacent
            && kind(text, items[next].marker) == kind(text, items[above].marker)
        {
            for item in &mut want[next..] {
                if item.list == from {
                    item.list = to;
                }
            }
        }
    }
    Some(want)
}

/// A marker's kind as a list continues it: its bullet, or the delimiter after its digits.
fn kind(text: &str, marker: usize) -> Option<u8> {
    text.bytes()
        .skip(marker)
        .find(|byte| !byte.is_ascii_digit())
}

/// Whether `at` is an empty item above a blank line: it ends at the blank line, and cannot hold
/// the siblings it adopts past it (Known limits).
pub(super) fn empty_above_blank(text: &str, items: &[Read], at: usize) -> bool {
    let end = line_end(text, items[at].marker);
    let below = text
        .get(end + 1..)
        .map(|below| &below[..line_end(below, 0)]);
    is_empty_item(text, items, at)
        && below.is_some_and(|line| line.trim_matches([' ', '\t', '>']).is_empty())
}

/// Whether every quote of `text` the edit does not delete still opens in `new`, as deep.
pub(super) fn quotes_kept(text: &str, edit: &ListEdit, new: &str) -> Result<(), String> {
    let (_, old_quotes) = oracle(text);
    let (_, new_quotes) = oracle(new);
    let deleted = |at: usize| {
        edit.hunks
            .iter()
            .any(|hunk| (hunk.range.start.get()..hunk.range.end.get()).contains(&at))
    };
    // A quote's nesting: the quotes holding its opener, itself included.
    let level = |quotes: &[Range<usize>], at: usize| {
        quotes.iter().filter(|quote| quote.contains(&at)).count()
    };
    for quote in old_quotes.iter().filter(|quote| !deleted(quote.start)) {
        let (to, nesting) = (landing(edit, quote.start), level(&old_quotes, quote.start));
        if !new_quotes
            .iter()
            .any(|new| new.contains(&to) && level(&new_quotes, new.start) == nesting)
        {
            return Err(format!(
                "quote at {} lost: {text:?} gave {new:?}",
                quote.start
            ));
        }
    }
    Ok(())
}

/// Whether `at`'s marker line holds nothing past its marker and its box.
pub(super) fn is_empty_item(text: &str, items: &[Read], at: usize) -> bool {
    let marker = items[at].marker;
    let rest = text[marker..line_end(text, marker)]
        .trim_start_matches(|c: char| !c.is_ascii_whitespace())
        .trim();
    ["[ ]", "[x]"]
        .iter()
        .find_map(|task| rest.strip_prefix(task))
        .unwrap_or(rest)
        .trim()
        .is_empty()
}
