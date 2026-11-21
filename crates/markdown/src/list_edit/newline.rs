//! `↩`: continue a list, a quote or an item's code, or leave an empty item (list editing spec, D2,
//! D3, D8).

use std::ops::Range;

use inkra_text::ByteOffset;

use super::shift::shifted;
use super::{Line, ListEdit, blanks, container_prefix, content_column, holder, hunk, lines_before};
use crate::Markdown;
use crate::items::{MarkerBlanks, column_after, marker_end};
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
    let prefix = |column| container_prefix(text, markdown, line.range.start, column);
    if in_code(markdown, &line) {
        // A code line keeps its blanks before its leaf text start.
        let gutter = line.gutter?;
        let held = holder.map_or(0, |at| content_column(text, markdown, at));
        let column = held.max(line.column(text, gutter.range.end.get()));
        return Some(split(text, &line, caret, &prefix(column)));
    }
    // Its line is a marker line: before its text start, the plain key (rule 9).
    if let Some(start) = empty_fallback(text, &line) {
        return (caret >= start).then(|| leave(&line));
    }
    if let Some(at) = holder {
        return Some(split(
            text,
            &line,
            caret,
            &prefix(content_column(text, markdown, at)),
        ));
    }
    let gutter = line.gutter?;
    match gutter.quotes.last() {
        // An empty quote line leaves its innermost quote; a lazy line is never empty.
        Some(&last) if gutter.quotes.len() == gutter.depth && line.is_blank(text) => {
            Some(ListEdit::replace(last.get()..line.range.end, String::new()))
        }
        _ => Some(split(text, &line, caret, &prefix(0))),
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
fn empty_fallback(text: &str, line: &Line<'_>) -> Option<usize> {
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
    let text_start = line.text_start();
    let typed = text[text_start..caret].trim_end_matches([' ', '\t']).len();
    let rest_blank = blanks(&text[caret..line.range.end]) == line.range.end - caret;
    if opens_code(markdown, line) {
        let column = content_column(text, markdown, item);
        let prefix = container_prefix(text, markdown, line.range.start, column);
        return ListEdit::replace(text_start + typed..caret, format!("\n{prefix}"));
    }
    let end = markdown.structure.items[item].end.get();
    let holds = lines_before(text, line.range.end + 1, end)
        .any(|held| !Line::at(text, markdown, held.start).is_blank(text));
    match (typed, rest_blank) {
        (0, true) if !holds => leave(line),
        (0, false) => above(text, markdown, line, item, caret),
        _ => continued(
            text,
            markdown,
            line,
            item,
            text_start + typed..caret,
            rest_blank,
        ),
    }
}

/// `↩` past an item's text, or on an empty marker line that holds lines: the next sibling, in
/// place of `blanks`, the blanks before the caret, or past the blank lines below when nothing
/// follows the caret, taking the lines the item holds below it (D2, rule 6, D7).
fn continued(
    text: &str,
    markdown: &Markdown,
    line: &Line<'_>,
    item: usize,
    blanks: Range<usize>,
    rest_blank: bool,
) -> ListEdit {
    let end = markdown.structure.items[item].end.get();
    let blank = |held: &Range<usize>| Line::at(text, markdown, held.start).is_blank(text);
    let mut below = lines_before(text, line.range.end + 1, end).peekable();
    let mut last_blank = None;
    while let Some(held) = below.next_if(blank) {
        last_blank = Some(held);
    }
    // An item begins with one blank line at most, so the new one follows them.
    let past_blanks = last_blank.filter(|_| rest_blank && below.peek().is_some());
    let (at, moved) = match past_blanks {
        Some(held) => (held.end..held.end, held.end + 1),
        None => (blanks, line.range.end + 1),
    };
    let (continuation, marker) = continuation(text, markdown, line, item, 1);
    // The text after the caret starts the new line, its blanks included; an empty line has none.
    let rest = if rest_blank {
        ""
    } else {
        &text[at.end..line.range.end]
    };
    let column = MarkerBlanks::at(&format!("{continuation}{rest}"), 0, marker).content_column();
    let old = content_column(text, markdown, item);
    // The caret lands at the new text start, past the blanks the text brought.
    let mut reached = column_after(0, continuation.as_bytes());
    let past = rest
        .bytes()
        .take_while(|&byte| {
            let before = reached;
            reached = column_after(reached, &[byte]);
            before < column && matches!(byte, b' ' | b'\t')
        })
        .count();
    let mut edit = ListEdit::replace(at, format!("\n{continuation}"));
    edit.caret = ByteOffset::new(edit.caret.get() + past);
    for (index, held) in lines_before(text, moved, end).enumerate() {
        let held = Line::at(text, markdown, held.start);
        if held.is_blank(text) {
            continue;
        }
        let lead = held.lead_end(text);
        let lazy = held.column(text, lead) < old
            || held
                .gutter
                .is_some_and(|gutter| gutter.quotes.len() < gutter.depth);
        // An empty marker line opens no paragraph for the lazy line right below to continue.
        if index == 0 && rest_blank && lazy {
            let prefix = container_prefix(text, markdown, held.range.start, column);
            edit.hunks.push(hunk(held.range.start..lead, prefix));
        } else if column > old {
            edit.hunks.push(shifted(&held, text, column - old));
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

/// The blanks before `caret`, back to the text start, replaced by `\n` and `prefix`; a line left
/// with no text keeps its quote part alone, trimmed (D2, rules 2 and 8).
fn split(text: &str, line: &Line<'_>, caret: usize, prefix: &str) -> ListEdit {
    let text_start = line.text_start();
    let typed = text[text_start..caret].trim_end_matches([' ', '\t']).len();
    let start = if typed == 0 {
        line.trimmed_quote_end(text)
    } else {
        text_start + typed
    };
    ListEdit::replace(start..caret, format!("\n{prefix}"))
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
