//! `↩`: continue a list, a quote or an item's code, or leave an empty item (list editing spec, D2,
//! D3, D8).

use std::iter;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::shift::shifted;
use super::{Line, ListEdit, blanks, container_prefix, content_column, holder, hunk};
use crate::Markdown;
use crate::items::{MarkerBlanks, marker_end};
use crate::spans::{first_from, last_before};
use crate::structure::Fallback;

/// The edit `↩` makes at `caret` in `text`, as `markdown`, its parse, reads it; `None` where no
/// list rule applies and the plain `\n` goes in. The caller gives a selection and marked text the
/// plain key (D2, rule 1).
#[must_use]
pub fn continue_list(text: &str, markdown: &Markdown, caret: ByteOffset) -> Option<ListEdit> {
    let caret = caret.get();
    let line = Line::at(text, markdown, caret);
    if opens_past(markdown, &line) || caret < line.text_start() {
        return None;
    }
    if let Some(item) = caret_item(markdown, &line) {
        return Some(on_marker_line(text, markdown, &line, item, caret));
    }
    let holder = holder(text, markdown, line.range.start);
    if in_code(markdown, &line) {
        // A code line keeps its blanks before its leaf text start.
        let gutter = line.gutter?;
        let held = holder.map_or(0, |at| content_column(text, markdown, at));
        let column = held.max(line.column(text, gutter.range.end.get()));
        return Some(split(text, markdown, &line, caret, column));
    }
    // Its line is a marker line: before its text start, the plain key (rule 9).
    if let Some(start) = empty_fallback(text, markdown, &line) {
        return (caret >= start).then(|| leave(&line));
    }
    if let Some(at) = holder {
        let column = content_column(text, markdown, at);
        return Some(split(text, markdown, &line, caret, column));
    }
    let gutter = line.gutter?;
    match gutter.quotes.last() {
        // An empty quote line leaves its innermost quote; a lazy line is never empty.
        Some(&last) if gutter.quotes.len() == gutter.depth && line.is_blank(text) => {
            Some(ListEdit::replace(last.get()..line.range.end, String::new()))
        }
        _ => Some(split(text, markdown, &line, caret, 0)),
    }
}

/// Whether an item or a quote opens on the line at or past its text start (D2, rule 10).
fn opens_past(markdown: &Markdown, line: &Line<'_>) -> bool {
    let structure = &markdown.structure;
    let past = ByteOffset::new(line.text_start());
    let item = first_from(&structure.items, past, |item| item.marker).map(|item| item.marker);
    let quote = first_from(&structure.quotes, past, |&quote| quote).copied();
    [item, quote]
        .into_iter()
        .flatten()
        .any(|at| at.get() <= line.range.end)
}

/// The innermost gutter item opening on the line: the last item whose marker lies in its item part.
fn caret_item(markdown: &Markdown, line: &Line<'_>) -> Option<usize> {
    let gutter = line.gutter?;
    gutter.item.as_ref().filter(|item| item.marked)?;
    last_before(&markdown.structure.items, gutter.range.end, |item| {
        item.marker
    })
}

/// Whether a code block touches the line.
fn in_code(markdown: &Markdown, line: &Line<'_>) -> bool {
    first_from(&markdown.code_blocks, line.range.start, |block| {
        block.range.end.get()
    })
    .is_some_and(|block| block.range.start.get() <= line.range.end)
}

/// Whether a code block opens on the line at or past its text start (D2, rule 3).
fn opens_code(markdown: &Markdown, line: &Line<'_>) -> bool {
    first_from(&markdown.code_blocks, line.text_start(), |block| {
        block.range.start.get()
    })
    .is_some_and(|block| block.range.start.get() <= line.range.end)
}

/// The text start of an empty marker the fallback reads past the line's quote part and leading
/// blanks, on a line where no item opens and no code lies (D8).
fn empty_fallback(text: &str, markdown: &Markdown, line: &Line<'_>) -> Option<usize> {
    // Front matter and HTML hold no markdown marker.
    let skipped = first_from(&markdown.structure.skipped, line.range.start, |range| {
        range.end.get()
    });
    if skipped.is_some_and(|range| range.start.get() <= line.range.start) {
        return None;
    }
    #[cfg(test)]
    FALLBACKS.set(FALLBACKS.get() + 1);
    let fallback = Fallback::read(text, &line.range, line.lead_end(text))?;
    let piece = &fallback.piece;
    let end = marker_end(piece, fallback.marker);
    let rest = &piece[end..piece.len() - 1];
    (blanks(rest) == rest.len()).then(|| fallback.in_text(end) + usize::from(!rest.is_empty()))
}

/// `↩` on a marker line at or past its text start (D2, rules 3, 5, 6 and 7).
fn on_marker_line(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    item: usize,
    caret: usize,
) -> ListEdit {
    let typed_end = line.typed_end(text, caret);
    if opens_code(markdown, line) {
        let column = content_column(text, markdown, item);
        let prefix = container_prefix(text, markdown, line.range.start, column);
        return ListEdit::replace(typed_end..caret, format!("\n{prefix}"));
    }
    let rest_blank = line.is_blank_past(text, caret);
    match (typed_end > line.text_start(), rest_blank) {
        (false, true) if held(text, markdown, line, item).next().is_none() => leave(line),
        (false, false) => above(text, markdown, line, item, caret),
        _ => continued(text, markdown, line, item, typed_end..caret),
    }
}

/// `↩` past an item's text, or on an empty marker line that holds lines: the next sibling in
/// place of `cut` and the blanks after it (D2, rule 6, D7).
fn continued(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    item: usize,
    cut: Range<usize>,
) -> ListEdit {
    let empty = line.is_blank_past(text, cut.end);
    let cut = cut.start..cut.end + blanks(&text[cut.end..line.range.end]);
    let (continuation, marker) = continuation(text, markdown, line, item, 1);
    let column = MarkerBlanks::at(&continuation, 0, marker).content_column();
    let shift = column.saturating_sub(content_column(text, markdown, item));
    let mut moved = moved(text, markdown, line, item, shift);
    let first = moved.next();
    // An item begins with one blank line at most, so an empty new one goes past them.
    let past_blanks = first
        .as_ref()
        .map(|held| held.range.start - 1)
        .filter(|&at| empty && at > line.range.end);
    let inserted = format!("\n{continuation}");
    let (mut hunks, landing) = match past_blanks {
        Some(at) => {
            let gone = (!cut.is_empty()).then(|| hunk(cut, String::new()));
            (
                gone.into_iter().chain([hunk(at..at, inserted)]).collect(),
                at,
            )
        }
        None => (vec![hunk(cut.clone(), inserted)], cut.end),
    };
    // An empty new line opens no paragraph for a lazy line right below to continue.
    let lazy = |held: &Line<'_>| held.lazy_prefix(text, markdown, column).filter(|_| empty);
    hunks.extend(first.and_then(|held| lazy(&held).or_else(|| shifted(&held, text, shift))));
    hunks.extend(moved.filter_map(|held| shifted(&held, text, shift)));
    ListEdit::new(hunks, landing)
}

/// The lines item `item` holds below `line` that a sibling continuing it can move: all of them
/// under a `shift`, else the first, which only a lazy prefix moves (D7).
fn moved<'t, 'm>(
    text: &'t str,
    markdown: &'m Markdown,
    line: &Line<'m>,
    item: usize,
    shift: usize,
) -> impl Iterator<Item = Line<'m>> + use<'t, 'm> {
    let reach = if shift == 0 { 1 } else { usize::MAX };
    held(text, markdown, line, item).take(reach)
}

/// The lines item `item` holds below `line`, blank ones left out.
fn held<'t, 'm>(
    text: &'t str,
    markdown: &'m Markdown,
    line: &Line<'m>,
    item: usize,
) -> impl Iterator<Item = Line<'m>> + use<'t, 'm> {
    let end = markdown.structure.items[item].end.get();
    line.below(text, markdown)
        .take_while(move |held| held.range.start < end)
        .filter(|held| !held.is_blank(text))
}

/// `↩` at the text start of an item with text: an empty sibling above it, or a blank line above
/// the first item of a list, where an empty item cannot interrupt a paragraph (D2, rule 7).
fn above(text: &str, markdown: &Markdown, line: &Line<'_>, item: usize, caret: usize) -> ListEdit {
    let start = line.range.start;
    let inserted = if has_previous_sibling(markdown, item) {
        format!("{}\n", continuation(text, markdown, line, item, 0).0)
    } else {
        format!("{}\n", &text[start..line.trimmed_quote_end(text)])
    };
    ListEdit::new(vec![hunk(start..start, inserted)], caret)
}

/// Whether item `at` has a sibling above it: the nearest item above in its list, found by walking
/// from the item before it up through its parents to its own parent (D1).
fn has_previous_sibling(markdown: &Markdown, at: usize) -> bool {
    let items = &markdown.structure.items;
    let mut walk = at.checked_sub(1);
    while let Some(above) = walk {
        if items[above].list == items[at].list {
            return true;
        }
        if items[at].parent.map(|parent| parent as usize) == Some(above) {
            return false;
        }
        walk = items[above].parent.map(|parent| parent as usize);
    }
    false
}

/// A sibling's marker line for item `at`, its number `step` past the item's, and its marker's
/// offset in it: the container prefix to its marker's column, its marker, the blanks after it or one
/// space past four columns, and an unchecked box for a task (D2, rule 6).
fn continuation(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    at: usize,
    step: u64,
) -> (String, usize) {
    let marker = markdown.structure.items[at].marker.get();
    let mut out = container_prefix(text, markdown, line.range.start, line.column(text, marker));
    let offset = out.len();
    let blanks = MarkerBlanks::at(text, line.range.start, marker);
    let digits = &text[marker..blanks.end - 1];
    match digits.parse::<u64>() {
        Ok(number) => {
            let number = (number + step).to_string();
            out.extend(std::iter::repeat_n(
                '0',
                digits.len().saturating_sub(number.len()),
            ));
            out.push_str(&number);
            out.push_str(&text[blanks.end - 1..blanks.end]);
        }
        Err(_) => out.push_str(&text[marker..blanks.end]),
    }
    if (1..=4).contains(&blanks.columns) {
        out.push_str(&text[blanks.end..blanks.end + blanks.len]);
    } else {
        out.push(' ');
    }
    let task = line
        .gutter
        .and_then(|gutter| gutter.item.as_ref())
        .and_then(|item| item.task.as_ref());
    if task.is_some() {
        out.push_str("[ ] ");
    }
    (out, offset)
}

/// The blanks before `caret`, back to the text start, replaced by `\n` and the container prefix
/// to `column`; a line left with no text keeps the quote part alone, trimmed, as the container
/// prefix writes it, so a lazy line keeps its quote too. A blank new line gives a lazy line below
/// its prefix (D2, rules 2 and 8).
fn split(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    caret: usize,
    column: usize,
) -> ListEdit {
    let typed_end = line.typed_end(text, caret);
    let prefix = container_prefix(text, markdown, line.range.start, column);
    let (start, left) = if typed_end == line.text_start() {
        let quote = container_prefix(text, markdown, line.range.start, 0);
        (line.range.start, quote.trim_end().to_owned())
    } else {
        (typed_end, String::new())
    };
    let lazy = line
        .below(text, markdown)
        .next()
        .filter(|_| line.is_blank_past(text, caret))
        .and_then(|below| below.lazy_prefix(text, markdown, column));
    let hunks = iter::once(hunk(start..caret, format!("{left}\n{prefix}")));
    ListEdit::new(hunks.chain(lazy).collect(), caret)
}

/// The empty item's line past its quote part, deleted: the list ends there (D3).
fn leave(line: &Line<'_>) -> ListEdit {
    ListEdit::replace(line.quote_end()..line.range.end, String::new())
}

#[cfg(test)]
thread_local! {
    pub(super) static FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod properties;
