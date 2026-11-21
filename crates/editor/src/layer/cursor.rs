//! A layer's chunk list during one publish: split at a point that moves, so each change puts its
//! pieces in place without shifting the whole list (code highlighting spec, S2b deviation 2).

use std::collections::VecDeque;
use std::ops::Range;

use inkra_text::ByteOffset;

use super::{Chunk, differing};

pub(super) struct Cursor<S> {
    /// The chunks before the point, in order.
    done: Vec<Chunk<S>>,
    /// The chunks from the point on, in order.
    todo: VecDeque<Chunk<S>>,
}

impl<S: Clone + PartialEq> Cursor<S> {
    pub(super) fn new(chunks: Vec<Chunk<S>>) -> Self {
        Self {
            done: Vec::with_capacity(chunks.len()),
            todo: chunks.into(),
        }
    }

    pub(super) fn finish(mut self) -> Vec<Chunk<S>> {
        self.done.extend(self.todo);
        self.done
    }

    /// Puts `new` in place of what its range holds, except a chunk computed later, which stays;
    /// with `fills` false, nothing takes that place. Returns the span of the marks that changed.
    pub(super) fn replace(&mut self, new: Chunk<S>, fills: bool) -> Option<Range<ByteOffset>> {
        let chunk = new.start..new.end();
        let at = new.published;
        while let Some(last) = self.done.pop_if(|last| last.end() > chunk.start) {
            self.todo.push_front(last);
        }
        while let Some(first) = self.todo.pop_front_if(|first| first.end() <= chunk.start) {
            self.done.push(first);
        }
        let mut olds = Vec::new();
        while let Some(old) = self.todo.pop_front_if(|old| old.start < chunk.end) {
            olds.push(old);
        }
        let before = marks(&olds, &chunk);
        let new = fills.then_some(new);
        let mut pieces = Vec::with_capacity(olds.len() + 2);
        let mut tail = None;
        let mut gap = chunk.start;
        for old in olds {
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
        let after = marks(&pieces, &chunk);
        for piece in pieces.into_iter().rev() {
            self.todo.push_front(piece);
        }
        differing(&before, &after)
    }
}

/// The marks of `chunks` inside `range`, clipped to it, with each style cloned.
fn marks<S: Clone>(chunks: &[Chunk<S>], range: &Range<ByteOffset>) -> Vec<(Range<ByteOffset>, S)> {
    chunks
        .iter()
        .flat_map(|chunk| chunk.marks_in(range.clone()))
        .map(|(mark, style)| (mark, style.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
