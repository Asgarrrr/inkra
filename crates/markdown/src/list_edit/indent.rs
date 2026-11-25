//! `⇥` and `⇧⇥`: indent and outdent the items a selection touches, each with its block (list
//! editing spec, D4, D7, D8).

use std::iter;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::shift::Moves;
use super::{Doc, Empty, Kind, Line, ListEdit};
use crate::Markdown;
use crate::items::Content;
use crate::spans::{line_ranges, line_start};

/// The edit `⇥` makes on the items `selection` touches, or `None` when it touches none; with no
/// hunks, none can move. Its caret is the selection's end: the app maps the selection (D9).
#[must_use]
pub fn indent_items(
    text: &str,
    markdown: &Markdown,
    selection: Range<ByteOffset>,
) -> Option<ListEdit> {
    // An empty fallback item's marker already sits where a child's parses once text arrives (D8).
    edit(Doc { text, markdown }, selection, |doc, touched, moves| {
        for run in touched.runs(doc) {
            indent_run(doc, run, moves);
        }
    })
}

/// The edit `⇧⇥` makes on the items `selection` touches, as `indent_items` returns it.
#[must_use]
pub fn outdent_items(
    text: &str,
    markdown: &Markdown,
    selection: Range<ByteOffset>,
) -> Option<ListEdit> {
    edit(Doc { text, markdown }, selection, |doc, touched, moves| {
        let mut counted = None;
        for run in touched.runs(doc) {
            counted = outdent_run(doc, run, &touched.items, counted, moves);
        }
        for (line, empty) in &touched.fallbacks {
            outdent_fallback(line, empty, moves);
        }
    })
}

/// The moves `command` makes on what `selection` touches, as one edit.
fn edit<'a>(
    doc: Doc<'a>,
    selection: Range<ByteOffset>,
    command: impl FnOnce(Doc<'a>, &Touched<'a>, &mut Moves),
) -> Option<ListEdit> {
    let touched = Touched::read(doc, &selection)?;
    let mut moves = Moves::default();
    command(doc, &touched, &mut moves);
    Some(ListEdit::new(moves.hunks(doc), selection.end.get()))
}

/// What the lines a selection touches give, by `⌘L`'s rule: each its caret item, or an empty
/// fallback item; none on a code line. An item moves with a touched ancestor only (D4, D8).
struct Touched<'a> {
    /// In document order.
    items: Vec<usize>,
    fallbacks: Vec<(Line<'a>, Empty)>,
}

impl<'a> Touched<'a> {
    fn read(doc: Doc<'a>, selection: &Range<ByteOffset>) -> Option<Self> {
        let first = line_start(doc.text, selection.start.get());
        let end = selection.end.get();
        let mut items = Vec::new();
        let mut fallbacks = Vec::new();
        for range in
            line_ranges(doc.text, first).take_while(|line| line.start == first || line.start < end)
        {
            let line = doc.line(range);
            if let Some(item) = line.item() {
                items.push(item);
            } else if line.in_code() {
            } else if let Some(empty) = line.empty_fallback() {
                fallbacks.push((line, empty));
            } else {
                items.extend(line.holder());
            }
        }
        if items.is_empty() && fallbacks.is_empty() {
            return None;
        }
        items.sort_unstable();
        items.dedup();
        let structure = &doc.markdown.structure;
        let touched = items.clone();
        let moves_with = |at: Option<usize>| {
            iter::successors(at, |&at| structure.items[at].parent())
                .any(|at| touched.binary_search(&at).is_ok())
        };
        items.retain(|&at| !moves_with(structure.items[at].parent()));
        fallbacks.retain(|(line, _)| !moves_with(line.holder()));
        Some(Self { items, fallbacks })
    }

    /// Its items in runs of consecutive siblings.
    fn runs(&self, doc: Doc<'_>) -> impl Iterator<Item = &[usize]> {
        self.items
            .chunk_by(move |&above, &below| doc.next_sibling(above) == Some(below))
    }
}

/// `⇥` on a run: its first item's previous sibling becomes its parent. Its markers land on that
/// sibling's content column, and join the list its last child ends its block with (D4).
fn indent_run(doc: Doc<'_>, run: &[usize], moves: &mut Moves) {
    let Some(parent) = doc.previous_sibling(run[0]) else {
        return;
    };
    let by = doc.content_column(parent).cast_signed() - doc.marker_column(run[0]).cast_signed();
    let items = &doc.markdown.structure.items;
    let joined = doc
        .last_child(parent)
        .filter(|&child| ends_block(doc, child, parent))
        .map(|child| Kind::at(doc.text, items[child].marker.get()));
    for (step, &at) in (0..).zip(run) {
        let marker = match (joined, Kind::at(doc.text, items[at].marker.get())) {
            (Some(kind), _) => kind.nth(step + 1),
            // A list that starts past 1 cannot interrupt its new parent's paragraph.
            (None, Kind::Number { delimiter, .. }) => Kind::first(delimiter).nth(step),
            (None, kind @ Kind::Bullet(_)) => kind.nth(0),
        };
        shift_item(doc, at, by, marker, moves);
    }
}

/// Whether only blank lines follow `child`'s block in `parent`'s, so a run placed after it joins
/// `child`'s list.
fn ends_block(doc: Doc<'_>, child: usize, parent: usize) -> bool {
    let items = &doc.markdown.structure.items;
    let end = items[parent].end.get();
    doc.line_at(items[child].end.get())
        .below()
        .take_while(|line| line.range.start < end)
        .all(|line| line.is_blank())
}

/// `⇧⇥` on a run: its items become siblings of their parent in place, with its kind. The rest of
/// the parent's block follows the last one at its content column, and the siblings there become
/// its children (D4). A later touched sibling moves on its own, so the rest stops at its block.
/// `counted` is the parent the run before outdented beside and the step it reached; returns this
/// run's.
pub(super) fn outdent_run(
    doc: Doc<'_>,
    run: &[usize],
    touched: &[usize],
    counted: Option<(usize, u64)>,
    moves: &mut Moves,
) -> Option<(usize, u64)> {
    let items = &doc.markdown.structure.items;
    let parent = items[run[0]].parent()?;
    let marker = |at: usize| items[at].marker.get();
    // On its parent's marker line, an item has no blanks of its own to cut (amendment 1).
    if line_start(doc.text, marker(run[0])) == line_start(doc.text, marker(parent)) {
        return counted;
    }
    let by = doc.marker_column(parent).cast_signed() - doc.marker_column(run[0]).cast_signed();
    let kind = Kind::at(doc.text, marker(parent));
    let mut step = counted
        .filter(|&(at, _)| at == parent)
        .map_or(1, |(_, step)| step);
    let mut column = 0;
    for &at in run {
        column = shift_item(doc, at, by, kind.nth(step), moves);
        step += 1;
    }
    let last = run[run.len() - 1];
    let by = column.cast_signed() - doc.content_column(parent).cast_signed();
    let next = touched
        .iter()
        .copied()
        .find(|&at| at > last && items[at].parent() == Some(parent));
    let end = next.map_or(items[parent].end.get(), |at| {
        line_start(doc.text, marker(at))
    });
    let rest = doc
        .line_at(items[last].end.get())
        .below()
        .take_while(|line| line.range.start < end);
    moves.lines(rest, by);
    // Its siblings below, or a child list of the parent right below it (amendment 3).
    let below = items.partition_point(|item| item.marker.get() < items[last].end.get());
    let adopted = items.get(below).is_some_and(|item| {
        let line = line_start(doc.text, item.marker.get());
        item.parent() == Some(parent)
            && (item.list() == items[last].list() || !doc.line_at(line - 1).is_blank())
    });
    if adopted {
        // Under a child list that ends its block, with nothing between, they join it, as `⇥`'s
        // run does (amendment 4).
        let next_to = doc
            .line_at(items[last].end.get())
            .below()
            .take_while(|line| line.range.start < line_start(doc.text, marker(below)))
            .all(|line| line.is_blank());
        let joined = doc
            .last_child(last)
            .filter(|&child| next_to && ends_block(doc, child, last))
            .map(|child| Kind::at(doc.text, marker(child)));
        adopt(doc, below..next.unwrap_or(items.len()), by, joined, moves);
    }
    Some((parent, step))
}

/// The list from `siblings.start` on, before `siblings.end`, which the rest of its old parent's
/// block moved by `by`: in the list
/// `joined` continues, else renumbered from 1 while its numbers ran in sequence, since a list that
/// starts past 1 cannot interrupt the paragraph of the item now above it (D4, D6).
fn adopt(doc: Doc<'_>, siblings: Range<usize>, by: isize, joined: Option<Kind>, moves: &mut Moves) {
    let items = &doc.markdown.structure.items;
    let mut sibling = Some(siblings.start);
    let mut next = None;
    for step in 0.. {
        let Some(at) = sibling.filter(|at| siblings.contains(at)) else {
            return;
        };
        let marker = match (joined, Kind::at(doc.text, items[at].marker.get())) {
            (Some(kind), _) => kind.nth(step + 1),
            (
                None,
                Kind::Number {
                    value, delimiter, ..
                },
            ) if next.is_none_or(|next| value == next) => {
                next = Some(value + 1);
                Kind::first(delimiter).nth(step)
            }
            _ => return,
        };
        let (_, shift) = rewrite(doc, at, by, marker, moves);
        moves.lines(doc.line_at(items[at].marker.get()).held(at), shift - by);
        sibling = doc.next_sibling(at);
    }
}

/// `⇧⇥` on an empty fallback item: it outdents to its holder's marker column, with its holder's
/// kind; with no holder, it stays (D8).
pub(super) fn outdent_fallback(line: &Line<'_>, empty: &Empty, moves: &mut Moves) {
    let doc = line.doc;
    let Some(holder) = line.holder() else {
        return;
    };
    let by = line
        .column(empty.marker)
        .saturating_sub(doc.marker_column(holder));
    moves.line(line.range.start, -by.cast_signed());
    let kind = Kind::at(doc.text, doc.markdown.structure.items[holder].marker.get());
    moves.marker(doc.text, empty.marker, kind.nth(1));
}

/// Item `at` moved `by` columns with its block, its marker rewritten as `marker`; its other lines
/// take the move of its content column (D7). Returns its new content column.
fn shift_item(doc: Doc<'_>, at: usize, by: isize, marker: String, moves: &mut Moves) -> usize {
    let (column, shift) = rewrite(doc, at, by, marker, moves);
    let line = doc.line_at(doc.markdown.structure.items[at].marker.get());
    moves.line(line.range.start, by);
    moves.lines(line.held(at), shift);
    column
}

/// Item `at`'s marker rewritten as `marker` on a line moved `by` columns: its new content column,
/// and how far that moved. The blanks after it count from its new end, so a tab there reads anew.
fn rewrite(
    doc: Doc<'_>,
    at: usize,
    by: isize,
    marker: String,
    moves: &mut Moves,
) -> (usize, isize) {
    let old = doc.marker(at);
    let end = doc
        .marker_column(at)
        .saturating_add_signed(by)
        .saturating_add(marker.len());
    let column = Content::column(end, &doc.text.as_bytes()[old.blanks.clone()]);
    moves.marker(
        doc.text,
        doc.markdown.structure.items[at].marker.get(),
        marker,
    );
    (
        column,
        column.cast_signed() - old.content_column.cast_signed(),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod properties;
