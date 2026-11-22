//! `↩`: continue a list, a quote or an item's code, or leave an empty item (list editing spec, D2,
//! D3, D8).

use std::iter;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::shift::shifted;
use super::{Doc, Line, ListEdit, blanks, hunk};
use crate::Markdown;
use crate::items::{Content, Marker, MarkerKind, column_after, marker_end};
use crate::spans::{last_before, touches};
use crate::structure::Fallback;

/// The edit `↩` makes at `caret`, or `None` for the plain `\n` (D2). A selection and marked text
/// take the plain key from the caller (rule 1).
#[must_use]
pub fn continue_list(text: &str, markdown: &Markdown, caret: ByteOffset) -> Option<ListEdit> {
    let caret = caret.get();
    let doc = Doc { text, markdown };
    let line = doc.line_at(caret);
    if opens_past(&line) || caret < line.text_start() {
        return None;
    }
    if let Some(item) = caret_item(&line) {
        return Some(on_marker_line(&line, item, caret));
    }
    let holder = doc.holder(line.range.start);
    if in_code(&line) {
        // A code line keeps its blanks before its leaf text start.
        let gutter = line.gutter?;
        let held = holder.map_or(0, |at| doc.content_column(at));
        let column = held.max(line.column(gutter.range.end.get()));
        return Some(split(&line, caret, column));
    }
    // Its line is a marker line: before its text start, the plain key (rule 9).
    if let Some(start) = empty_fallback(&line) {
        return (caret >= start).then(|| leave(&line));
    }
    if let Some(at) = holder {
        return Some(split(&line, caret, doc.content_column(at)));
    }
    let gutter = line.gutter?;
    match gutter.quotes.last() {
        // An empty quote line leaves its innermost quote; a lazy line is never empty.
        Some(&last) if gutter.quotes.len() == gutter.depth && line.is_blank() => {
            Some(ListEdit::replace(last.get()..line.range.end, String::new()))
        }
        _ => Some(split(&line, caret, 0)),
    }
}

/// Whether an item or a quote opens on the line at or past its text start (D2, rule 10).
fn opens_past(line: &Line<'_>) -> bool {
    let structure = &line.doc.markdown.structure;
    let past = line.text_start()..=line.range.end;
    touches(&structure.items, past.clone(), |item| {
        item.marker.get()..item.marker.get()
    }) || touches(&structure.quotes, past, |quote| quote.get()..quote.get())
}

/// The innermost gutter item opening on the line: the last item whose marker lies in its item part.
fn caret_item(line: &Line<'_>) -> Option<usize> {
    let gutter = line.gutter?;
    gutter.item.as_ref().filter(|item| item.marked)?;
    last_before(
        &line.doc.markdown.structure.items,
        gutter.range.end,
        |item| item.marker,
    )
}

/// Whether a code block touches the line.
fn in_code(line: &Line<'_>) -> bool {
    touches(
        &line.doc.markdown.code_blocks,
        line.range.start..=line.range.end,
        |block| block.range.start.get()..block.range.end.get(),
    )
}

/// Whether a code block opens on the line at or past its text start (D2, rule 3).
fn opens_code(line: &Line<'_>) -> bool {
    touches(
        &line.doc.markdown.code_blocks,
        line.text_start()..=line.range.end,
        |block| block.range.start.get()..block.range.start.get(),
    )
}

/// The text start of an empty marker the fallback reads past the line's quote part and leading
/// blanks, on a line where no item opens and no code lies (D8).
fn empty_fallback(line: &Line<'_>) -> Option<usize> {
    // Front matter and HTML hold no markdown marker.
    let lines = line.range.start..=line.range.end;
    if touches(&line.doc.markdown.structure.skipped, lines, |range| {
        range.start.get()..range.end.get()
    }) {
        return None;
    }
    let fallback = Fallback::read(line.doc.text, &line.range, line.lead_end())?;
    let piece = &fallback.piece;
    let end = marker_end(piece, fallback.marker);
    let rest = &piece[end..piece.len() - 1];
    (blanks(rest) == rest.len()).then(|| fallback.in_text(end) + usize::from(!rest.is_empty()))
}

/// `↩` on a marker line at or past its text start (D2, rules 3, 5, 6 and 7).
fn on_marker_line(line: &Line<'_>, item: usize, caret: usize) -> ListEdit {
    let doc = line.doc;
    let typed_end = line.typed_end(caret);
    if opens_code(line) {
        let prefix = doc.container_prefix(line.range.start, doc.content_column(item));
        return ListEdit::replace(typed_end..caret, format!("\n{prefix}"));
    }
    let text_before = typed_end > line.text_start();
    let gesture = if text_before {
        Gesture::Continue
    } else if !line.is_blank_past(caret) {
        Gesture::Above
    } else if held(line, item).next().is_some() {
        Gesture::Continue
    } else {
        Gesture::Leave
    };
    match gesture {
        Gesture::Leave => leave(line),
        Gesture::Above => above(line, item, caret),
        Gesture::Continue => continued(line, item, typed_end..caret),
    }
}

/// What `↩` does on a marker line at or past its text start.
enum Gesture {
    /// An empty item that holds no line leaves its list (D3).
    Leave,
    /// At the text start of an item with text, a line goes above it (rule 7).
    Above,
    /// Past an item's text, or on an empty marker line that holds lines, the next sibling (rule 6).
    Continue,
}

/// The next sibling, in place of `cut` and the blanks after it (D2, rule 6, D7).
fn continued(line: &Line<'_>, item: usize, cut: Range<usize>) -> ListEdit {
    let empty = line.is_blank_past(cut.end);
    let cut = cut.start..cut.end + blanks(&line.doc.text[cut.end..line.range.end]);
    let sibling = continuation(line, item, 1);
    let column = sibling.content_column;
    let shift = column.saturating_sub(line.doc.content_column(item));
    let mut held = held(line, item);
    let first = held.next();
    // An item begins with one blank line at most, so an empty new one goes past them.
    let past_blanks = first
        .as_ref()
        .map(|held| held.range.start - 1)
        .filter(|&at| empty && at > line.range.end);
    let inserted = format!("\n{}", sibling.line);
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
    let lazy = |held: &Line<'_>| held.lazy_prefix(column).filter(|_| empty);
    hunks.extend(first.and_then(|held| lazy(&held).or_else(|| shifted(&held, shift))));
    // Only a shift moves the lines past the first, so only a shift reads them (D7).
    if shift > 0 {
        hunks.extend(held.filter_map(|held| shifted(&held, shift)));
    }
    ListEdit::new(hunks, landing)
}

/// The lines item `item` holds below `line`, blank ones left out.
fn held<'a>(line: &Line<'a>, item: usize) -> impl Iterator<Item = Line<'a>> + use<'a> {
    let end = line.doc.markdown.structure.items[item].end.get();
    line.below()
        .take_while(move |held| held.range.start < end)
        .filter(|held| !held.is_blank())
}

/// `↩` at the text start of an item with text: an empty sibling above it, or a blank line above
/// the first item of a list, where an empty item cannot interrupt a paragraph (D2, rule 7).
fn above(line: &Line<'_>, item: usize, caret: usize) -> ListEdit {
    let start = line.range.start;
    let inserted = if has_previous_sibling(line.doc.markdown, item) {
        format!("{}\n", continuation(line, item, 0).line)
    } else {
        format!("{}\n", &line.doc.text[start..line.trimmed_quote_end()])
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
        if items[at].parent() == Some(above) {
            return false;
        }
        walk = items[above].parent();
    }
    false
}

/// A sibling's marker line: its text, and the column its content starts at.
struct Continuation {
    line: String,
    content_column: usize,
}

/// The marker line of a sibling of item `at`, numbered `step` past it (D2, rule 6).
fn continuation(line: &Line<'_>, at: usize, step: u64) -> Continuation {
    let Doc { text, markdown } = line.doc;
    let at = markdown.structure.items[at].marker.get();
    let mut out = line.doc.container_prefix(line.range.start, line.column(at));
    let marker = Marker::at(text, line.range.start, at);
    let number = match &marker.kind {
        MarkerKind::Number { digits } => text[digits.clone()]
            .parse::<u64>()
            .ok()
            .map(|number| (digits.clone(), (number + step).to_string())),
        MarkerKind::Bullet => None,
    };
    match number {
        Some((digits, number)) => {
            out.extend(iter::repeat_n(
                '0',
                digits.len().saturating_sub(number.len()),
            ));
            out.push_str(&number);
            out.push_str(&text[digits.end..marker.end]);
        }
        None => out.push_str(&text[at..marker.end]),
    }
    let blanks = match &marker.content {
        Content::Blanks(blanks) => &text[blanks.clone()],
        Content::Empty | Content::Indented => " ",
    };
    let after = column_after(0, out.as_bytes());
    let (_, content_column) = Content::read(after, out.len(), blanks.as_bytes());
    out.push_str(blanks);
    let task = line
        .gutter
        .and_then(|gutter| gutter.item.as_ref())
        .and_then(|item| item.task.as_ref());
    if task.is_some() {
        out.push_str("[ ] ");
    }
    Continuation {
        line: out,
        content_column,
    }
}

/// `\n` and the container prefix to `column`, in place of the blanks before `caret` (D2, rules 2
/// and 8).
fn split(line: &Line<'_>, caret: usize, column: usize) -> ListEdit {
    let doc = line.doc;
    let typed_end = line.typed_end(caret);
    let prefix = doc.container_prefix(line.range.start, column);
    // Not the line's own quote part, as `above` copies: a lazy line shows fewer `>` than it is in.
    let (start, left) = if typed_end == line.text_start() {
        let quote = doc.container_prefix(line.range.start, 0);
        (line.range.start, quote.trim_end().to_owned())
    } else {
        (typed_end, String::new())
    };
    let lazy = line
        .below()
        .next()
        .filter(|_| line.is_blank_past(caret))
        .and_then(|below| below.lazy_prefix(column));
    let hunks = iter::once(hunk(start..caret, format!("{left}\n{prefix}")));
    ListEdit::new(hunks.chain(lazy).collect(), caret)
}

/// The empty item's line past its quote part, deleted: the list ends there (D3).
fn leave(line: &Line<'_>) -> ListEdit {
    ListEdit::replace(line.quote_end()..line.range.end, String::new())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod properties;
