//! `↩`: continue a list, a quote or an item's code, or leave an empty item (list editing spec, D2,
//! D3, D8).

use std::ops::Range;

use inkra_text::ByteOffset;

use super::shift::shifted;
use super::{Line, ListEdit, blanks, container_prefix, content_column, holder, hunk};
use crate::Markdown;
use crate::items::{MarkerBlanks, marker_end};
use crate::structure::innermost_item;

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
    let past = line.text_start()..=line.range.end;
    let structure = &markdown.structure;
    let item = structure
        .items
        .partition_point(|item| item.marker.get() < *past.start());
    let quote = structure
        .quotes
        .partition_point(|quote| quote.get() < *past.start());
    structure
        .items
        .get(item)
        .is_some_and(|item| past.contains(&item.marker.get()))
        || structure
            .quotes
            .get(quote)
            .is_some_and(|quote| past.contains(&quote.get()))
}

/// The innermost gutter item opening on the line: the last item whose marker lies in its item part.
fn caret_item(markdown: &Markdown, line: &Line<'_>) -> Option<usize> {
    let gutter = line.gutter?;
    gutter.item.as_ref().filter(|item| item.marked)?;
    let items = &markdown.structure.items;
    items
        .partition_point(|item| item.marker < gutter.range.end)
        .checked_sub(1)
}

/// Whether a code block touches the line.
fn in_code(markdown: &Markdown, line: &Line<'_>) -> bool {
    let blocks = &markdown.code_blocks;
    let at = blocks.partition_point(|block| block.range.start.get() <= line.range.end);
    at.checked_sub(1)
        .is_some_and(|at| blocks[at].range.end.get() >= line.range.start)
}

/// Whether a code block opens on the line at or past its text start (D2, rule 3).
fn opens_code(markdown: &Markdown, line: &Line<'_>) -> bool {
    let blocks = &markdown.code_blocks;
    let at = blocks.partition_point(|block| block.range.start.get() < line.text_start());
    blocks
        .get(at)
        .is_some_and(|block| block.range.start.get() <= line.range.end)
}

/// The text start of an empty marker the fallback reads on a line where no item opens and no code
/// lies: one pass over the line past its quote part and leading blanks, padded so its tab stops
/// fall where the line's do (D8).
fn empty_fallback(text: &str, markdown: &Markdown, line: &Line<'_>) -> Option<usize> {
    // Front matter and HTML hold no markdown marker.
    let skipped = &markdown.structure.skipped;
    let at = skipped.partition_point(|range| range.end.get() < line.range.start);
    if skipped
        .get(at)
        .is_some_and(|range| range.start.get() <= line.range.start)
    {
        return None;
    }
    #[cfg(test)]
    FALLBACKS.set(FALLBACKS.get() + 1);
    let from = line.lead_end(text);
    let pad = line.column(text, from) % 4;
    let piece = format!("{:pad$}{}\n", "", &text[from..line.range.end]);
    let marker = innermost_item(&piece)?;
    let end = marker_end(&piece, marker);
    let rest = &piece[end..piece.len() - 1];
    (blanks(rest) == rest.len()).then(|| from + end - pad + usize::from(!rest.is_empty()))
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
    let end = markdown.structure.items[item].end.get();
    let holds = line
        .below(text, markdown)
        .take_while(|held| held.range.start < end)
        .any(|held| !held.is_blank(text));
    let rest_blank = line.is_blank_past(text, caret);
    match (typed_end > line.text_start(), rest_blank) {
        (false, true) if !holds => leave(line),
        (false, false) => above(text, markdown, line, item, caret),
        _ => continued(text, markdown, line, item, typed_end..caret, rest_blank),
    }
}

/// `↩` past an item's text, or on an empty marker line that holds lines: the next sibling, in
/// place of `cut`, the blanks before the caret and after it, or past the blank lines below when
/// nothing follows the caret, taking the lines the item holds below it (D2, rule 6, D7).
fn continued(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    item: usize,
    cut: Range<usize>,
    rest_blank: bool,
) -> ListEdit {
    let cut = cut.start..cut.end + blanks(&text[cut.end..line.range.end]);
    let end = markdown.structure.items[item].end.get();
    let blank = |held: &Line<'_>| held.is_blank(text);
    let mut below = line
        .below(text, markdown)
        .take_while(|held| held.range.start < end)
        .peekable();
    let mut last_blank = None;
    while let Some(held) = below.next_if(blank) {
        last_blank = Some(held);
    }
    let (continuation, marker) = continuation(text, markdown, line, item, 1);
    let inserted = format!("\n{continuation}");
    // An item begins with one blank line at most, so the new one follows them.
    let past_blanks = last_blank.filter(|_| rest_blank && below.peek().is_some());
    let (mut edit, moved) = match past_blanks {
        Some(held) => {
            let held = held.range;
            let caret = held.end - cut.len() + inserted.len();
            let gone = (!cut.is_empty()).then(|| hunk(cut.clone(), String::new()));
            let hunks = gone.into_iter().chain([hunk(held.end..held.end, inserted)]);
            let caret = ByteOffset::new(caret);
            (
                ListEdit {
                    hunks: hunks.collect(),
                    caret,
                },
                held.end + 1,
            )
        }
        None => (ListEdit::replace(cut, inserted), line.range.end + 1),
    };
    let column = MarkerBlanks::at(&continuation, 0, marker).content_column();
    let shift = column.saturating_sub(content_column(text, markdown, item));
    // Only a shift reaches past the first line the new item takes.
    let reach = if shift == 0 { 1 } else { usize::MAX };
    let held = line
        .below(text, markdown)
        .skip_while(|held| held.range.start < moved)
        .take_while(|held| held.range.start < end)
        .inspect(|_| {
            #[cfg(test)]
            MOVED_READ.set(MOVED_READ.get() + 1);
        })
        .filter(|held| !held.is_blank(text))
        .take(reach);
    for (index, held) in held.enumerate() {
        // An empty marker line opens no paragraph for the lazy line right below to continue.
        if index == 0 && rest_blank && held.is_lazy(text, markdown) {
            let prefix = container_prefix(text, markdown, held.range.start, column);
            edit.hunks
                .push(hunk(held.range.start..held.lead_end(text), prefix));
        } else if shift > 0 {
            edit.hunks.push(shifted(&held, text, shift));
        }
    }
    edit
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
    ListEdit {
        caret: ByteOffset::new(caret + inserted.len()),
        hunks: vec![hunk(start..start, inserted)],
    }
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
    let mut edit = ListEdit::replace(start..caret, format!("{left}\n{prefix}"));
    if line.is_blank_past(text, caret) {
        edit.hunks.extend(line.lazy_below(text, markdown, column));
    }
    edit
}

/// The empty item's line past its quote part, deleted: the list ends there (D3).
fn leave(line: &Line<'_>) -> ListEdit {
    ListEdit::replace(line.quote_end()..line.range.end, String::new())
}

#[cfg(test)]
thread_local! {
    pub(super) static FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The lines a continuation's walk over the lines it takes reads.
    pub(super) static MOVED_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod properties;
