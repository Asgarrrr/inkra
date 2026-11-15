use std::ops::Range;

use inkra_text::{Buffer, ByteOffset, EditMark, Splice};

/// A layer could not follow the buffer's edits and holds nothing until it is published again.
#[derive(Debug, PartialEq, Eq)]
pub struct Lost;

/// A feature's styled ranges over one buffer, published chunk by chunk.
pub struct Layer<S> {
    at: EditMark,
    /// Sorted, disjoint and non-empty.
    chunks: Vec<Chunk<S>>,
}

struct Chunk<S> {
    start: ByteOffset,
    len: usize,
    /// The edit mark its marks were computed at: an older publish never replaces them.
    published: EditMark,
    /// What its publisher passed, opaque here; `None` once a splice changed its bytes or a publish
    /// cut it.
    tag: Option<u64>,
    /// Relative to `start`: sorted, disjoint, non-empty and inside `0..len`.
    marks: Vec<(Range<usize>, S)>,
}

/// A chunk as `Layer::publish` takes it: its range, its tag and the marks inside it.
type Published<S> = (Range<ByteOffset>, u64, Vec<(Range<ByteOffset>, S)>);

impl<S: Clone + PartialEq> Layer<S> {
    #[must_use]
    pub fn new(at: EditMark) -> Self {
        Self {
            at,
            chunks: Vec::new(),
        }
    }

    #[must_use]
    pub fn at(&self) -> EditMark {
        self.at
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// The marks intersecting `range`, clipped to it, in order.
    pub fn marks_in(
        &self,
        range: Range<ByteOffset>,
    ) -> impl Iterator<Item = (Range<ByteOffset>, &S)> + '_ {
        let first = self
            .chunks
            .partition_point(|chunk| chunk.end() <= range.start);
        // An empty range intersects nothing, not even a mark around it.
        let chunks = if range.is_empty() {
            &[]
        } else {
            &self.chunks[first..]
        };
        chunks
            .iter()
            .take_while(move |chunk| chunk.start < range.end)
            .flat_map(move |chunk| chunk.marks_in(range.clone()))
    }

    /// Each chunk's range and tag, in order.
    pub fn chunks(&self) -> impl Iterator<Item = (Range<ByteOffset>, Option<u64>)> + '_ {
        self.chunks
            .iter()
            .map(|chunk| (chunk.start..chunk.end(), chunk.tag))
    }

    /// Brings the layer to `buffer`'s edit mark, shifting its marks through every splice since.
    ///
    /// # Errors
    /// `Lost` when the buffer's log dropped a splice since: the layer is cleared.
    pub fn catch_up(&mut self, buffer: &Buffer) -> Result<(), Lost> {
        let Some(splices) = buffer.edits_since(self.at) else {
            self.clear();
            self.at = buffer.edit_mark();
            return Err(Lost);
        };
        for splice in splices {
            self.shift(splice);
        }
        self.at = buffer.edit_mark();
        Ok(())
    }

    /// Maps the chunks `splice` reaches and moves the start of every chunk after them.
    fn shift(&mut self, splice: Splice) {
        let first = self
            .chunks
            .partition_point(|chunk| chunk.end() <= splice.at);
        let last = self
            .chunks
            .partition_point(|chunk| chunk.start < splice.at + splice.removed_len);
        let reached: Vec<_> = self
            .chunks
            .drain(first..last)
            .filter_map(|chunk| map_chunk(chunk, splice))
            .collect();
        for chunk in &mut self.chunks[first..] {
            chunk.start = chunk.start + splice.inserted_len - splice.removed_len;
        }
        self.chunks.splice(first..first, reached);
    }

    /// Empties each range of `removed`, then replaces everything inside each chunk of `chunks`, a
    /// range with its tag and its marks. All are computed at the edit mark `at` and mapped through
    /// the edits since (D6), except where a chunk computed later lies: that one stays. Inside the
    /// batch, a later change wins over an earlier one. A chunk holds its tag while it stays whole
    /// and its bytes unchanged. `Ok(true)` when a change moved the marks inside its mapped range.
    ///
    /// # Errors
    /// `Lost` when the layer could not catch up or the buffer's log dropped a splice since `at`:
    /// the layer is cleared and the whole batch dropped.
    ///
    /// # Panics
    /// If a range is reversed, a chunk's marks are unsorted, overlapping or outside it, or once
    /// mapped, a range is past the end of `buffer` or a bound splits a char.
    pub fn publish(
        &mut self,
        buffer: &Buffer,
        removed: Vec<Range<ByteOffset>>,
        chunks: Vec<Published<S>>,
        at: EditMark,
    ) -> Result<bool, Lost> {
        for range in &removed {
            check::<S>(range, &[]);
        }
        for (range, _, marks) in &chunks {
            check(range, marks);
        }
        self.catch_up(buffer)?;
        let Some(splices) = buffer.edits_since(at) else {
            self.clear();
            return Err(Lost);
        };
        let splices: Vec<_> = splices.collect();
        let removed = removed
            .into_iter()
            .map(|range| (Chunk::new(range, at, None, []), false));
        let chunks = chunks
            .into_iter()
            .map(|(range, tag, marks)| (Chunk::new(range, at, Some(tag), marks), true));
        let mut changed = false;
        for (new, fills) in removed.chain(chunks) {
            changed = self.replace(buffer, new, fills, &splices, changed);
        }
        Ok(changed)
    }

    /// Maps `new` through `splices` and puts it in place of what its range holds; with `fills`
    /// false, nothing takes that place. Returns whether the marks there changed, or `true` at once
    /// when `changed` already says an earlier change of the batch moved some.
    fn replace(
        &mut self,
        buffer: &Buffer,
        new: Chunk<S>,
        fills: bool,
        splices: &[Splice],
        changed: bool,
    ) -> bool {
        let at = new.published;
        let Some(new) = splices
            .iter()
            .copied()
            .try_fold(new, map_chunk)
            .filter(|new| new.len != 0)
        else {
            return changed;
        };
        new.check(buffer);
        let chunk = new.start..new.end();
        let new = fills.then_some(new);
        let first = self.chunks.partition_point(|old| old.end() <= chunk.start);
        let last = self.chunks.partition_point(|old| old.start < chunk.end);
        let before: Option<Vec<_>> = (!changed).then(|| {
            self.marks_in(chunk.clone())
                .map(|(mark, style)| (mark, style.clone()))
                .collect()
        });
        let mut pieces = Vec::new();
        let mut tail = None;
        let mut gap = chunk.start;
        for old in self.chunks.drain(first..last) {
            if old.published > at {
                pieces.extend(new.as_ref().and_then(|new| new.piece(gap..old.start)));
                gap = old.end();
                pieces.push(old);
            } else {
                pieces.extend(old.piece(old.start..chunk.start));
                tail = old.piece(chunk.end..old.end());
            }
        }
        pieces.extend(new.as_ref().and_then(|new| new.piece(gap..chunk.end)));
        pieces.extend(tail);
        self.chunks.splice(first..first, pieces);
        before.is_none_or(|before| {
            let after = self.marks_in(chunk);
            !after.eq(before.iter().map(|(mark, style)| (mark.clone(), style)))
        })
    }
}

impl<S: Clone> Chunk<S> {
    fn new(
        range: Range<ByteOffset>,
        published: EditMark,
        tag: Option<u64>,
        marks: impl IntoIterator<Item = (Range<ByteOffset>, S)>,
    ) -> Self {
        let start = range.start;
        Self {
            start,
            len: range.end - start,
            published,
            tag,
            marks: marks
                .into_iter()
                .filter(|(mark, _)| !mark.is_empty())
                .map(|(mark, style)| (mark.start - start..mark.end - start, style))
                .collect(),
        }
    }

    fn end(&self) -> ByteOffset {
        self.start + self.len
    }

    /// As `Layer::marks_in`, for a non-empty `range`.
    fn marks_in(&self, range: Range<ByteOffset>) -> impl Iterator<Item = (Range<ByteOffset>, &S)> {
        let first = self
            .marks
            .partition_point(|(mark, _)| self.start + mark.end <= range.start);
        self.marks[first..]
            .iter()
            .map(|(mark, style)| (self.start + mark.start..self.start + mark.end, style))
            .take_while(move |(mark, _)| mark.start < range.end)
            .map(move |(mark, style)| (mark.start.max(range.start)..mark.end.min(range.end), style))
    }

    /// The part of the chunk inside `range`, `None` when empty; only the whole chunk keeps its tag.
    fn piece(&self, range: Range<ByteOffset>) -> Option<Self> {
        (range.start < range.end).then(|| {
            let marks = self
                .marks_in(range.clone())
                .map(|(mark, style)| (mark, style.clone()));
            let tag = self.tag.filter(|_| range == (self.start..self.end()));
            Self::new(range, self.published, tag, marks)
        })
    }

    fn check(&self, buffer: &Buffer) {
        let chunk = self.start..self.end();
        assert!(
            chunk.end <= buffer.len(),
            "chunk {chunk:?} is past the end of the buffer"
        );
        let marks = self
            .marks
            .iter()
            .flat_map(|(mark, _)| [self.start + mark.start, self.start + mark.end]);
        for bound in [chunk.start, chunk.end].into_iter().chain(marks) {
            assert!(
                buffer.text().is_char_boundary(bound.get()),
                "{bound} in chunk {chunk:?} splits a char"
            );
        }
    }
}

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
fn map_chunk<S: Clone>(chunk: Chunk<S>, splice: Splice) -> Option<Chunk<S>> {
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
    let marks = chunk.marks.into_iter().flat_map(|(mark, style)| {
        let (left, right) = map_mark(start + mark.start..start + mark.end, splice);
        [left, right]
            .into_iter()
            .flatten()
            .map(move |piece| (piece, style.clone()))
    });
    Some(Chunk::new(range, chunk.published, tag, marks))
}

/// What holds of a publish at any edit mark; the bounds are checked once mapped (`Chunk::check`).
fn check<S>(chunk: &Range<ByteOffset>, marks: &[(Range<ByteOffset>, S)]) {
    assert!(chunk.start <= chunk.end, "chunk {chunk:?} is reversed");
    let mut floor = chunk.start;
    for (mark, _) in marks {
        assert!(
            [floor, mark.start, mark.end, chunk.end].is_sorted(),
            "mark {mark:?} is reversed, overlaps the one before or lies outside chunk {chunk:?}"
        );
        floor = mark.end;
    }
}

#[cfg(test)]
mod tests;
