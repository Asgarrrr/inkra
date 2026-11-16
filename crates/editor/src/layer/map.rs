//! How a chunk and its marks follow one splice (decoration layer spec, D3).

use std::ops::Range;

use inkra_text::{ByteOffset, Splice};

use super::Chunk;

/// D3: what is left of `mark` after `splice`, left and right of the inserted bytes, which it never
/// covers. The two pieces are one when they touch.
fn map_mark(
    mark: Range<ByteOffset>,
    splice: Splice,
) -> (Option<Range<ByteOffset>>, Option<Range<ByteOffset>>) {
    let removed_end = splice.at + splice.removed_len;
    let left = (mark.start < splice.at).then(|| mark.start..mark.end.min(splice.at));
    let kept = mark.start.max(removed_end);
    let right = (kept < mark.end).then(|| {
        kept - splice.removed_len + splice.inserted_len
            ..mark.end - splice.removed_len + splice.inserted_len
    });
    match (left, right) {
        (Some(left), Some(right)) if left.end == right.start => (Some(left.start..right.end), None),
        pieces => pieces,
    }
}

/// As `map_mark` on the chunk's range, except that a splice strictly inside it grows it (decoration
/// layer spec, deviation 5). Either way at most one piece is left, and its marks follow `map_mark`;
/// it keeps its tag unless the splice removed one of its bytes or inserted strictly inside it.
pub(super) fn map_chunk<S: Clone>(chunk: Chunk<S>, splice: Splice) -> Option<Chunk<S>> {
    let (start, end) = (chunk.start, chunk.end());
    let tag = chunk
        .tag
        .filter(|_| !(start < splice.at + splice.removed_len && splice.at < end));
    let range = if start < splice.at && splice.at + splice.removed_len < end {
        start..end - splice.removed_len + splice.inserted_len
    } else {
        let (left, right) = map_mark(start..end, splice);
        left.or(right)?
    };
    let marks = chunk.marks.into_iter().flat_map(|mark| {
        let (left, right) = map_mark(mark.range(start), splice);
        [left, right]
            .into_iter()
            .flatten()
            .map(move |piece| (piece, mark.style.clone()))
    });
    Some(Chunk::new(range, chunk.published, tag, marks))
}

/// As `map_chunk`, but a splice that removes none of the chunk's bytes and inserts nowhere strictly
/// inside it moves only its start, or nothing (code highlighting spec, deviation 12).
pub(super) fn follow<S: Clone>(mut chunk: Chunk<S>, splice: Splice) -> Option<Chunk<S>> {
    if reaches(&chunk, splice) {
        map_chunk(chunk, splice)
    } else {
        if splice.at + splice.removed_len <= chunk.start {
            chunk.start = chunk.start + splice.inserted_len - splice.removed_len;
        }
        Some(chunk)
    }
}

/// Whether `splice` removes one of the chunk's bytes or inserts strictly inside it.
pub(super) fn reaches<S: Clone>(chunk: &Chunk<S>, splice: Splice) -> bool {
    chunk.start < splice.at + splice.removed_len && splice.at < chunk.end()
}

/// `new` mapped through `splices`, as `follow` maps it; `None` once nothing of it is left, or with
/// `skip`, once a splice reaches it (code highlighting spec, S2b deviation 1).
pub(super) fn follow_all<S: Clone>(
    mut new: Chunk<S>,
    splices: &[Splice],
    skip: bool,
) -> Option<Chunk<S>> {
    // Moves only the start until a splice reaches the chunk: most of a landing's chunks never are.
    let reached = splices.iter().position(|&splice| {
        let reached = reaches(&new, splice);
        if !reached && splice.at + splice.removed_len <= new.start {
            new.start = new.start + splice.inserted_len - splice.removed_len;
        }
        reached
    });
    let new = match reached {
        None => new,
        Some(_) if skip => return None,
        Some(first) => splices[first..].iter().copied().try_fold(new, follow)?,
    };
    (new.len != 0).then_some(new)
}
