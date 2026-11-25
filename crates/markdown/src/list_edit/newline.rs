//! `↩`: continue a list, a quote or an item's code, or leave an empty item (list editing spec, D2,
//! D3, D8).

use std::iter;
use std::num::NonZeroUsize;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::indent::{outdent_fallback, outdent_run};
use super::shift::{Moves, shifted};
use super::{Doc, Kind, Line, ListEdit, blanks, hunk};
use crate::Markdown;
use crate::items::{Content, Marker, column_after};
use crate::spans::{line_start, touches};

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
    if let Some(item) = line.item() {
        return Some(on_marker_line(&line, item, caret));
    }
    let holder = line.holder();
    if line.in_code() {
        // A code line keeps its blanks before its leaf text start.
        let gutter = line.gutter?;
        let held = holder.map_or(0, |at| doc.content_column(at));
        let column = held.max(line.column(gutter.range.end.get()));
        return Some(split(&line, caret, column));
    }
    // Its line is a marker line: before its text start, the plain key (rule 9).
    if let Some(empty) = line.empty_fallback() {
        if caret < empty.text_start {
            return None;
        }
        // Nested under a holder, it outdents as `⇧⇥` does (D3, D8).
        if holder.is_none() {
            return Some(leave(&line));
        }
        let mut moves = Moves::default();
        outdent_fallback(&line, &empty, &mut moves);
        return Some(ListEdit::new(moves.hunks(doc), empty.text_start));
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

/// Whether a code block opens on the line at or past its text start (D2, rule 3).
fn opens_code(line: &Line<'_>) -> bool {
    touches(
        &line.doc.markdown.code_blocks,
        line.text_start()..=line.range.end,
        |block| block.range.start.get()..block.range.start.get(),
    )
}

/// `↩` on a marker line at or past its text start (D2, rules 3, 5, 6 and 7).
fn on_marker_line(line: &Line<'_>, item: usize, caret: usize) -> ListEdit {
    let doc = line.doc;
    let typed_end = line.typed_end(caret);
    if opens_code(line) {
        let prefix = line.container_prefix(doc.content_column(item));
        return ListEdit::replace(typed_end..caret, format!("\n{prefix}"));
    }
    if typed_end > line.text_start() {
        // Past an item's text, the next sibling (rule 6).
        continued(line, item, typed_end..caret)
    } else if !line.is_blank_past(caret) {
        // At the text start of an item with text, a line goes above it (rule 7).
        above(line, item, caret)
    } else if line.held(item).next().is_some() {
        // An empty marker line that holds lines takes the next sibling (rule 6).
        continued(line, item, typed_end..caret)
    } else {
        exit(line, item)
    }
}

/// `↩` on an empty item that holds no line: it leaves its list at the top level, and outdents one
/// level when nested (D3).
fn exit(line: &Line<'_>, item: usize) -> ListEdit {
    let doc = line.doc;
    let Some(parent) = doc.markdown.structure.items[item].parent() else {
        return leave(line);
    };
    let marker = doc.markdown.structure.items[item].marker.get();
    // On its parent's marker line, it cannot outdent in place: its marker goes (amendment 1).
    if line_start(doc.text, doc.markdown.structure.items[parent].marker.get()) == line.range.start {
        return ListEdit::replace(marker..line.range.end, String::new());
    }
    let mut moves = Moves::default();
    outdent_run(doc, &[item], &mut moves);
    ListEdit::new(moves.hunks(doc), line.text_start())
}

/// The next sibling, in place of `cut` and the blanks after it (D2, rule 6, D7).
fn continued(line: &Line<'_>, item: usize, cut: Range<usize>) -> ListEdit {
    let empty = line.is_blank_past(cut.end);
    let cut = cut.start..cut.end + blanks(&line.doc.text[cut.end..line.range.end]);
    let sibling = continuation(line, item, 1);
    let column = sibling.content_column;
    let shift = NonZeroUsize::new(column.saturating_sub(line.doc.content_column(item)));
    let mut held = line.held(item);
    let first = held.next();
    // An item begins with one blank line at most, so an empty new one goes past them.
    let landing = first
        .as_ref()
        .filter(|_| empty)
        .map_or(cut.end, |held| held.range.start - 1);
    let gone = (!cut.is_empty()).then(|| hunk(cut, String::new()));
    let inserted = hunk(landing..landing, format!("\n{}", sibling.line));
    let mut hunks: Vec<_> = gone.into_iter().chain([inserted]).collect();
    // An empty new line opens no paragraph for a lazy line right below to continue.
    let lazy = |held: &Line<'_>| held.lazy_prefix(column).filter(|_| empty);
    let moved = first.and_then(|held| lazy(&held).or_else(|| shift.map(|by| shifted(&held, by))));
    hunks.extend(moved);
    // Only a shift moves the lines past the first, so only a shift reads them (D7).
    if let Some(by) = shift {
        hunks.extend(held.map(|held| shifted(&held, by)));
    }
    ListEdit::new(hunks, landing)
}

/// `↩` at the text start of an item with text: an empty sibling above it, or a blank line above
/// the first item of a list, where an empty item cannot interrupt a paragraph (D2, rule 7).
fn above(line: &Line<'_>, item: usize, caret: usize) -> ListEdit {
    let start = line.range.start;
    let inserted = if line.doc.previous_sibling(item).is_some() {
        format!("{}\n", continuation(line, item, 0).line)
    } else {
        format!("{}\n", &line.doc.text[start..line.trimmed_quote_end()])
    };
    ListEdit::new(vec![hunk(start..start, inserted)], caret)
}

/// A sibling's marker line: its text, and the column its content starts at.
struct Continuation {
    line: String,
    content_column: usize,
}

/// The marker line of a sibling of item `item`, numbered `step` past it (D2, rule 6).
fn continuation(line: &Line<'_>, item: usize, step: u64) -> Continuation {
    let Doc { text, markdown } = line.doc;
    let at = markdown.structure.items[item].marker.get();
    let mut out = line.container_prefix(line.column(at));
    let marker = Marker::at(text, line.range.start, at);
    out.push_str(&Kind::at(text, at).nth(step));
    let blanks = match &marker.content {
        Content::Blanks(blanks) => &text[blanks.clone()],
        Content::Empty | Content::Indented => " ",
    };
    let content_column = Content::column(column_after(0, out.as_bytes()), blanks.as_bytes());
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
    let typed_end = line.typed_end(caret);
    let prefix = line.container_prefix(column);
    // Not the line's own quote part, as `above` copies: a lazy line shows fewer `>` than it is in.
    let (start, left) = if typed_end == line.text_start() {
        let quote = line.container_prefix(0);
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
