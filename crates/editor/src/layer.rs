use std::ops::Range;

use inkra_text::{Buffer, ByteOffset, EditMark};

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
    /// Relative to `start`: sorted, disjoint, non-empty and inside `0..len`.
    marks: Vec<(Range<usize>, S)>,
}

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

    /// Brings the layer to `buffer`'s edit mark.
    ///
    /// # Errors
    /// `Lost` when the buffer was edited since: the layer is cleared.
    pub fn catch_up(&mut self, buffer: &Buffer) -> Result<(), Lost> {
        let current = buffer.edit_mark();
        if self.at == current {
            return Ok(());
        }
        self.clear();
        self.at = current;
        Err(Lost)
    }

    /// Replaces everything inside `chunk` with `marks`, computed at the edit mark `at`.
    /// `Ok(true)` when the marks inside `chunk` changed.
    ///
    /// # Errors
    /// `Lost` when the layer could not catch up; the publish is dropped.
    ///
    /// # Panics
    /// If `chunk` is past the end of `buffer`, a bound splits a char, or `marks` are unsorted,
    /// overlapping or outside `chunk`.
    pub fn publish(
        &mut self,
        buffer: &Buffer,
        chunk: Range<ByteOffset>,
        marks: Vec<(Range<ByteOffset>, S)>,
        at: EditMark,
    ) -> Result<bool, Lost> {
        check(buffer, &chunk, &marks);
        self.catch_up(buffer)?;
        if at != self.at || chunk.is_empty() {
            return Ok(false);
        }
        let before: Vec<_> = self
            .marks_in(chunk.clone())
            .map(|(mark, style)| (mark, style.clone()))
            .collect();
        let first = self.chunks.partition_point(|old| old.end() <= chunk.start);
        let last = self.chunks.partition_point(|old| old.start < chunk.end);
        let overlapped = &self.chunks[first..last];
        let head = overlapped
            .first()
            .and_then(|head| head.piece(head.start..chunk.start));
        let tail = overlapped
            .last()
            .and_then(|tail| tail.piece(chunk.end..tail.end()));
        let new = Chunk::new(chunk.clone(), marks);
        self.chunks
            .splice(first..last, head.into_iter().chain([new]).chain(tail));
        let after = self.marks_in(chunk);
        Ok(!after.eq(before.iter().map(|(mark, style)| (mark.clone(), style))))
    }
}

impl<S: Clone> Chunk<S> {
    fn new(
        range: Range<ByteOffset>,
        marks: impl IntoIterator<Item = (Range<ByteOffset>, S)>,
    ) -> Self {
        let start = range.start;
        Self {
            start,
            len: range.end - start,
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

    /// The part of the chunk inside `range`, `None` when empty.
    fn piece(&self, range: Range<ByteOffset>) -> Option<Self> {
        (range.start < range.end).then(|| {
            let marks = self
                .marks_in(range.clone())
                .map(|(mark, style)| (mark, style.clone()));
            Self::new(range, marks)
        })
    }
}

fn check<S>(buffer: &Buffer, chunk: &Range<ByteOffset>, marks: &[(Range<ByteOffset>, S)]) {
    assert!(
        [chunk.start, chunk.end, buffer.len()].is_sorted(),
        "chunk {chunk:?} is reversed or past the end of the buffer"
    );
    let mut floor = chunk.start;
    for (mark, _) in marks {
        assert!(
            [floor, mark.start, mark.end, chunk.end].is_sorted(),
            "mark {mark:?} is reversed, overlaps the one before or lies outside chunk {chunk:?}"
        );
        floor = mark.end;
    }
    let bounds = marks.iter().flat_map(|(mark, _)| [mark.start, mark.end]);
    for bound in [chunk.start, chunk.end].into_iter().chain(bounds) {
        assert!(
            buffer.text().is_char_boundary(bound.get()),
            "{bound} in chunk {chunk:?} splits a char"
        );
    }
}

#[cfg(test)]
mod tests;
