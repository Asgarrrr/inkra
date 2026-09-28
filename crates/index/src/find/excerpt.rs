use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use super::highlight::{boundaries, light, prefix};
use super::plain::{LINE_BYTES, Plain};
use super::preview::preview;
use super::terms::{Terms, each_word};
use super::{Group, NoteMatch};
use crate::index::IndexSnapshot;
use crate::window::around;
use crate::words::{NoteQuery, WORD_MAX_CHARS, cap};

/// A text row whose first match lies past this has no excerpt: finding it reads every byte before.
pub(super) const EXCERPT_SCAN_BYTES: usize = 1 << 20;

/// The second line of a quick-open row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    pub text: String,
    /// Byte ranges into `text` of matched word prefixes, whole graphemes, ascending, disjoint.
    pub highlights: Vec<Range<usize>>,
}

/// A text row's line around its first match, else the first prose line, markup dropped.
/// `None` if unloaded, stale, cancelled, unmatched, prose-less, or past `EXCERPT_SCAN_BYTES`.
#[must_use]
pub fn excerpt(
    snapshot: &IndexSnapshot,
    found: &NoteMatch,
    query: &NoteQuery,
    cancel: &AtomicBool,
) -> Option<Excerpt> {
    let (id, text) = snapshot.corpus().text(found.rank.order)?;
    if id != found.id || cancel.load(Ordering::Relaxed) {
        return None;
    }
    if found.group == Group::Name || query.is_empty() {
        let stem = snapshot.names().words().of(found.rank.order);
        preview(
            text,
            stem.filter(|word| word.in_stem).map(|word| word.folded),
            cancel,
        )
    } else {
        around_match(text, &Terms::new(query), cancel)
    }
}

pub(super) fn around_match(text: &str, terms: &Terms<'_>, cancel: &AtomicBool) -> Option<Excerpt> {
    let scanned = &text[..text.floor_char_boundary(EXCERPT_SCAN_BYTES)];
    let mut window: Option<Range<usize>> = None;
    let mut matched: Vec<(Range<usize>, &str)> = Vec::new();
    each_word(scanned, cancel, |word| {
        let spelled = &scanned[word.clone()];
        matched.extend(terms.matching(spelled).map(|term| (word.clone(), term)));
        if window.is_none() && !matched.is_empty() {
            // A megabyte word gives a window around its first chars, not around it.
            let head = cap(spelled, WORD_MAX_CHARS).len();
            window = Some(around(text, word.start..word.start + head));
        }
        window.as_ref().map_or(usize::MAX, |window| window.end)
    })?;
    let window = window?;
    let at_line_start = window.start == 0 || text[..window.start].ends_with('\n');
    let from = if at_line_start {
        window.end - text[window.clone()].trim_start().len()
    } else {
        window.start
    };
    let context = line_around(text, &window);
    let context_at_line_start = context.start == 0 || text[..context.start].ends_with('\n');
    let shown = Plain::new(
        &text[context.clone()],
        context_at_line_start,
        from - context.start..window.end - context.start,
    );
    let bounds = boundaries(&shown.text);
    let mut highlights = Vec::new();
    for (word, term) in matched {
        let word = shown.map(word.start - from)..shown.map(word.end.min(window.end) - from);
        // A word of the markup, like an ordered item's number, is not shown.
        if !word.is_empty() {
            light(&mut highlights, prefix(&shown.text, &bounds, word, term));
        }
    }
    Some(Excerpt {
        text: shown.text,
        highlights,
    })
}

/// Widened by up to `LINE_BYTES` each side in its line: a delimiter's partner may lie outside.
fn line_around(text: &str, window: &Range<usize>) -> Range<usize> {
    let before = text.ceil_char_boundary(window.start.saturating_sub(LINE_BYTES));
    let after = text.floor_char_boundary(window.end + LINE_BYTES);
    let start = text[before..window.start]
        .rfind('\n')
        .map_or(before, |at| before + at + 1);
    let end = text[window.end..after]
        .find('\n')
        .map_or(after, |at| window.end + at);
    start..end
}
