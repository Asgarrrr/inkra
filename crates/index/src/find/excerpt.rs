use std::borrow::Cow;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use super::highlight::{boundaries, light, prefix};
use super::plain::{LINE_BYTES, Plain};
use super::preview::preview;
use super::{Group, NoteMatch};
use crate::index::IndexSnapshot;
use crate::window::around;
use crate::words::{NoteQuery, WORD_MAX_CHARS, cap, fold, word_spans};

/// A text row whose first match lies past this has no excerpt: finding it reads every byte before.
pub(super) const EXCERPT_SCAN_BYTES: usize = 1 << 20;
pub(super) const CANCEL_EVERY: usize = 64 << 10;

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

pub(super) struct Terms<'q> {
    pub(super) terms: &'q [Box<str>],
    /// Indexed by a lowercase ASCII byte.
    pub(super) starts: [bool; 128],
}

impl<'q> Terms<'q> {
    pub(super) fn new(query: &'q NoteQuery) -> Self {
        let mut starts = [false; 128];
        for term in query.terms() {
            if let Some(&first) = term.as_bytes().first()
                && let Some(start) = starts.get_mut(usize::from(first))
            {
                *start = true;
            }
        }
        Self {
            terms: query.terms(),
            starts,
        }
    }

    fn matching<'w>(&self, word: &'w str) -> impl Iterator<Item = &'q str> + use<'q, 'w> {
        // An ASCII word folds to its lowercase, so its first byte rules out most words unfolded.
        let skipped = word.is_ascii()
            && word
                .as_bytes()
                .first()
                .is_some_and(|&first| !self.starts[usize::from(first.to_ascii_lowercase())]);
        let folded: Option<Cow<'w, str>> = (!skipped).then(|| fold(word));
        self.terms
            .iter()
            .filter(move |term| {
                folded
                    .as_deref()
                    .is_some_and(|word| word.starts_with(&***term))
            })
            .map(|term| &**term)
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

/// Stops before a word at the bound `each` last returned; `None` once `cancel` is set.
pub(super) fn each_word(
    text: &str,
    cancel: &AtomicBool,
    mut each: impl FnMut(Range<usize>) -> usize,
) -> Option<()> {
    let mut until = usize::MAX;
    // A word running into a block's end may go on in the next block.
    let mut open: Option<Range<usize>> = None;
    let mut from = 0;
    while from < text.len() && (from < until || open.is_some()) {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let to = text.ceil_char_boundary(from + CANCEL_EVERY);
        for (at, word) in word_spans(&text[from..to]) {
            let word = from + at..from + at + word.len();
            let word = match open.take() {
                Some(head) if head.end == word.start => head.start..word.end,
                Some(head) => {
                    until = each(head);
                    word
                }
                None => word,
            };
            if word.start >= until {
                return Some(());
            }
            if word.end == to {
                open = Some(word);
            } else {
                until = each(word);
            }
        }
        from = to;
    }
    if let Some(word) = open {
        each(word);
    }
    Some(())
}
