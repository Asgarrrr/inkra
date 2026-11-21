//! Which lines of the previous layout a new one carries (incremental layout spec, D5).

use std::cmp::Ordering;
use std::ops::Range;
use std::rc::Rc;

use inkra_markdown::{Conceal, changed, is_active};
use inkra_text::{Bias, ByteOffset, Splice};

use super::line::{LineLayout, Placed};
use super::{DocumentLayout, LayoutInputs};

/// The previous layout's lines, and the new text's dirty lines.
pub(super) struct Carry<'p> {
    old: &'p [Placed],
    /// Every splice since the previous layout as one; an empty one when none.
    edit: Splice,
    /// Whole lines in the new text, sorted, merged, each end on its last line's end.
    dirty: Vec<Range<ByteOffset>>,
    next_dirty: usize,
    next_old: usize,
}

/// `None` when a source cannot answer, or an input every line reads changed: the whole note is dirty.
/// A selection change dirties the lines of the conceals it flips, after the other sources, since a
/// flip on a dirty line needs nothing more.
pub(super) fn carry<'p>(
    inputs: &LayoutInputs<'_>,
    previous: &'p DocumentLayout,
) -> Option<Carry<'p>> {
    let buffer = inputs.buffer;
    // First: a mark from another buffer panics in `edits_since`.
    if previous.buffer != buffer.id()
        || !previous.same_settings(inputs)
        || previous.margin != inputs.margin
    {
        return None;
    }
    let recoloured = inputs
        .decorations
        .changed_since(previous.shaped_at.1, buffer)?;
    let spliced = Splice::compose(buffer.edits_since(previous.at)?);
    let edit = spliced.unwrap_or(Splice {
        at: ByteOffset::default(),
        removed_len: 0,
        inserted_len: 0,
    });
    let text = buffer.text();
    let mut dirty: Vec<_> =
        spliced
            .map(|edit| changed(&previous.markdown, inputs.markdown, edit))
            .into_iter()
            .flatten()
            .chain(previous.marked.as_ref().map(|marked| {
                edit.map(marked.start, Bias::Left)..edit.map(marked.end, Bias::Right)
            }))
            .chain(inputs.marked.cloned())
            .chain(recoloured)
            .map(|range| whole_lines(text, &range))
            .collect();
    merge(&mut dirty);
    let mut carry = Carry {
        old: &previous.lines,
        edit,
        dirty,
        next_dirty: 0,
        next_old: 0,
    };
    let flipped = carry.flipped(
        (&previous.markdown.conceals, &previous.selection),
        (&inputs.markdown.conceals, inputs.selection),
    );
    if !flipped.is_empty() {
        let lines = flipped
            .iter()
            .map(|&marker| whole_lines(text, &(marker..marker)));
        carry.dirty.extend(lines);
        merge(&mut carry.dirty);
    }
    Some(carry)
}

/// Sorts `dirty` and merges the ranges that meet.
fn merge(dirty: &mut Vec<Range<ByteOffset>>) {
    dirty.sort_unstable_by_key(|range| range.start);
    dirty.dedup_by(|next, kept| {
        let meets = next.start <= kept.end;
        if meets {
            kept.end = kept.end.max(next.end);
        }
        meets
    });
}

/// From the start of `range.start`'s line to the end of `range.end`'s, without its `\n`.
fn whole_lines(text: &str, range: &Range<ByteOffset>) -> Range<ByteOffset> {
    let (start, end) = (range.start.get(), range.end.get());
    let start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[end..]
        .find('\n')
        .map_or(text.len(), |newline| end + newline);
    ByteOffset::new(start)..ByteOffset::new(end)
}

impl<'p> Carry<'p> {
    /// The previous layout's line for the line at `at`, unless that line is dirty. Called with
    /// each line's start, in order.
    pub(super) fn clean(&mut self, at: ByteOffset) -> Option<&'p Rc<LineLayout>> {
        while self
            .dirty
            .get(self.next_dirty)
            .is_some_and(|dirty| dirty.end < at)
        {
            self.next_dirty += 1;
        }
        if self
            .dirty
            .get(self.next_dirty)
            .is_some_and(|dirty| dirty.start <= at)
        {
            return None;
        }
        let was = self.was(at);
        while self.old[self.next_old].start < was {
            self.next_old += 1;
        }
        let old = &self.old[self.next_old];
        // Outside the dirty lines, every byte and every `\n` is one the previous text held.
        debug_assert_eq!(old.start, was);
        Some(&old.line)
    }

    /// Where `offset`, on a clean line, stood in the previous text.
    fn was(&self, offset: ByteOffset) -> ByteOffset {
        if offset < self.edit.at {
            offset
        } else {
            offset + self.edit.removed_len - self.edit.inserted_len
        }
    }

    /// The markers, in the new text, of the conceals on clean lines that reveal under one
    /// selection and not the other (D9). Clean lines hold the same conceals, at the same markers,
    /// in both parses (`changed`), so a marker revealed on one side only is a flip. An element can
    /// still span the edit, so each side reads its own parse.
    fn flipped(
        &self,
        (old, before): (&[Conceal], &Range<ByteOffset>),
        (new, after): (&[Conceal], &Range<ByteOffset>),
    ) -> Vec<ByteOffset> {
        let mut was = self
            .revealed(old, before, |marker| self.edit.map(marker, Bias::Left))
            .peekable();
        let mut is = self.revealed(new, after, |marker| marker).peekable();
        let mut flipped = Vec::new();
        // Both are sorted: conceals are, and mapping through one splice keeps their order.
        loop {
            let order = match (was.peek(), is.peek()) {
                (Some(was), Some(is)) => was.cmp(is),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => return flipped,
            };
            match order {
                Ordering::Less => flipped.extend(was.next()),
                Ordering::Greater => flipped.extend(is.next()),
                Ordering::Equal => {
                    was.next();
                    is.next();
                }
            }
        }
    }

    /// The markers, placed in the new text by `place`, of the conceals on clean lines that
    /// `selection` reveals. A selection touches few elements, so few reach the dirty lookup.
    fn revealed<'c>(
        &'c self,
        conceals: &'c [Conceal],
        selection: &'c Range<ByteOffset>,
        place: impl Fn(ByteOffset) -> ByteOffset + 'c,
    ) -> impl Iterator<Item = ByteOffset> + 'c {
        conceals
            .iter()
            .filter(|conceal| !is_active(conceal, selection))
            .map(move |conceal| place(conceal.marker.start))
            .filter(|&marker| {
                let next = self.dirty.partition_point(|dirty| dirty.end < marker);
                self.dirty
                    .get(next)
                    .is_none_or(|dirty| dirty.start > marker)
            })
    }
}
