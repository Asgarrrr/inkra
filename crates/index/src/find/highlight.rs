use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::names::NamesSnapshot;
use crate::words::{NoteQuery, fold};

/// Every grapheme start of `text`, then its length.
pub(super) fn boundaries(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect()
}

/// From `word`'s first grapheme to the first grapheme end whose run folds to a start of `term`.
pub(super) fn prefix(text: &str, bounds: &[usize], word: Range<usize>, term: &str) -> Range<usize> {
    let after = bounds.partition_point(|&at| at <= word.start);
    let start = after
        .checked_sub(1)
        .and_then(|at| bounds.get(at))
        .map_or(0, |&at| at);
    let ends = bounds.get(after..).unwrap_or_default();
    let end = ends
        .iter()
        .find(|&&end| fold(&text[word.start..end.min(word.end)]).starts_with(term))
        .or_else(|| ends.iter().find(|&&end| end >= word.end))
        .map_or(text.len(), |&end| end);
    start..end
}

/// Each name word a term starts, lit in whole path graphemes: ascending, touching ranges merged.
pub(super) fn name(names: &NamesSnapshot, order: usize, query: &NoteQuery) -> Vec<Range<usize>> {
    let path = names.path(order);
    let mut bounds = None;
    let mut lit: Vec<Range<usize>> = Vec::new();
    for word in names.words().of(order) {
        for term in query.terms() {
            if !word.folded.starts_with(&**term) {
                continue;
            }
            let bounds = bounds.get_or_insert_with(|| boundaries(path));
            light(&mut lit, prefix(path, bounds, word.bytes.clone(), term));
        }
    }
    lit
}

pub(super) fn light(lit: &mut Vec<Range<usize>>, range: Range<usize>) {
    match lit.last_mut() {
        Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
        _ => lit.push(range),
    }
}
