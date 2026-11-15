//! Which lines of the previous layout a new one carries (incremental layout spec, D5).

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

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
pub(super) fn carry<'p>(
    inputs: &LayoutInputs<'_>,
    previous: &'p DocumentLayout,
) -> Option<Carry<'p>> {
    let buffer = inputs.buffer;
    // First: a mark from another buffer panics in `edits_since`.
    if previous.buffer != buffer.id()
        || previous.style != *inputs.style
        || !Arc::ptr_eq(&previous.theme, inputs.theme)
        || previous.wrap_width != inputs.wrap_width
        || previous.margin != inputs.margin
        || previous.code_block_wrap != inputs.code_block_wrap
    {
        return None;
    }
    let recoloured = inputs
        .decorations
        .changed_since(previous.shaped_at.1, buffer)?;
    let hull = inkra_highlight::dirty(buffer.edits_since(previous.at)?);
    let old_len = previous.lines.last()?.end().get();
    let edit = hull.as_ref().map_or(
        Splice {
            at: ByteOffset::default(),
            removed_len: 0,
            inserted_len: 0,
        },
        |hull| Splice {
            at: hull.start,
            inserted_len: hull.end - hull.start,
            removed_len: hull.end - hull.start + old_len - buffer.len().get(),
        },
    );
    let text = buffer.text();
    let mut dirty: Vec<_> =
        hull.map(|_| changed(&previous.markdown, inputs.markdown, edit))
            .into_iter()
            .chain(previous.marked.as_ref().map(|marked| {
                edit.map(marked.start, Bias::Left)..edit.map(marked.end, Bias::Right)
            }))
            .chain(inputs.marked.cloned())
            .chain(recoloured)
            .map(|range| whole_lines(text, &range))
            .collect();
    dirty.sort_unstable_by_key(|range| range.start);
    dirty.dedup_by(|next, kept| {
        let meets = next.start <= kept.end;
        if meets {
            kept.end = kept.end.max(next.end);
        }
        meets
    });
    let carry = Carry {
        old: &previous.lines,
        edit,
        dirty,
        next_dirty: 0,
        next_old: 0,
    };
    let flips = carry.flips(
        &inputs.markdown.conceals,
        &previous.selection,
        inputs.selection,
    );
    (!flips).then_some(carry)
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
        let was = self.was(at, at);
        while self.old[self.next_old].start < was {
            self.next_old += 1;
        }
        let old = &self.old[self.next_old];
        // Outside the dirty lines, every byte and every `\n` is one the previous text held.
        debug_assert_eq!(old.start, was);
        Some(&old.line)
    }

    /// Where `offset` stood in the previous text, for an item at `side` on a clean line: the edit
    /// moved it only if the item is past the edit's start.
    fn was(&self, side: ByteOffset, offset: ByteOffset) -> ByteOffset {
        if side < self.edit.at {
            offset
        } else {
            offset + self.edit.removed_len - self.edit.inserted_len
        }
    }

    /// Whether a conceal on a clean line reveals under one selection and not the other. Every such
    /// conceal is one the previous parse held, before the edit or after it.
    fn flips(
        &self,
        conceals: &[Conceal],
        before: &Range<ByteOffset>,
        after: &Range<ByteOffset>,
    ) -> bool {
        let mut dirty = self.dirty.iter().peekable();
        conceals.iter().any(|conceal| {
            let marker = conceal.marker.start;
            while dirty.next_if(|dirty| dirty.end < marker).is_some() {}
            if dirty.peek().is_some_and(|dirty| dirty.start <= marker) {
                return false;
            }
            let element = &conceal.element;
            let old = Conceal {
                element: self.was(marker, element.start)..self.was(marker, element.end),
                ..conceal.clone()
            };
            is_active(&old, before) != is_active(conceal, after)
        })
    }
}
