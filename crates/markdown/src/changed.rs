//! Which part of a note's layout an edit can change, from the parses before and after it
//! (incremental layout spec, D6, mapped form).

use std::cmp::Ordering;
use std::ops::Range;

use inkra_text::{Bias, ByteOffset, Splice};

use crate::{CodeBlock, Conceal, Gutter, Heading, Markdown, Span};

/// The bytes of `new`'s text outside which every line reads the same items as before `edit`, which
/// turned `old`'s text into `new`'s; in no order, the edit's own bytes included. Each vector splits
/// into a common prefix before the edit, a common suffix after it, and a middle. A middle item that
/// `edit` maps onto a new one is clean; every other one answers the text whose lines read it.
#[must_use]
pub fn changed(old: &Markdown, new: &Markdown, edit: Splice) -> Vec<Range<ByteOffset>> {
    let mut dirty = vec![edit.at..edit.at + edit.inserted_len];
    middle(&old.spans, &new.spans, edit, &mut dirty);
    middle(&old.conceals, &new.conceals, edit, &mut dirty);
    middle(&old.gutters, &new.gutters, edit, &mut dirty);
    middle(&old.headings, &new.headings, edit, &mut dirty);
    middle(&old.code_blocks, &new.code_blocks, edit, &mut dirty);
    dirty
}

fn middle<T: Item>(old: &[T], new: &[T], edit: Splice, dirty: &mut Vec<Range<ByteOffset>>) {
    let removed_end = edit.at + edit.removed_len;
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(old, new)| old.extent().end <= edit.at && old.same(new, |offset, _| offset))
        .count();
    let shift = |offset: ByteOffset, _| offset + edit.inserted_len - edit.removed_len;
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(old, new)| old.extent().start >= removed_end && old.same(new, shift))
        .count();
    let (old, new) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let moved = |offset, bias| edit.map(offset, bias);
    // A walk in step: equal items keep their order, so each line's clean items keep theirs.
    let (mut was, mut is) = (0, 0);
    while was < old.len() || is < new.len() {
        let order = match (old.get(was), new.get(is)) {
            (Some(old), Some(new)) if old.same(new, moved) => {
                was += 1;
                is += 1;
                continue;
            }
            (Some(old), Some(new)) => moved(old.key(), Bias::Right).cmp(&new.key()),
            (Some(_), None) => Ordering::Less,
            (None, _) => Ordering::Greater,
        };
        match order {
            Ordering::Less => dirty.push(old[was].moved_reach(edit)),
            Ordering::Greater => dirty.push(new[is].reach()),
            Ordering::Equal => T::differ(&old[was], &new[is], edit, dirty),
        }
        was += usize::from(order != Ordering::Greater);
        is += usize::from(order != Ordering::Less);
    }
}

/// What the layout reads of one parse item.
trait Item {
    /// Holds every offset of the item.
    fn extent(&self) -> Range<ByteOffset>;

    /// The offset its vector is sorted by.
    fn key(&self) -> ByteOffset {
        self.extent().start
    }

    /// The text whose lines read the item.
    fn reach(&self) -> Range<ByteOffset> {
        self.extent()
    }

    /// The new text whose lines read the item, which `edit` moved.
    fn moved_reach(&self, edit: Splice) -> Range<ByteOffset> {
        let reach = self.reach();
        edit.map(reach.start, Bias::Left)..edit.map(reach.end, Bias::Right)
    }

    /// Whether the layout reads `self`, each offset moved by `map`, as it reads `new`.
    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool;

    /// The new text whose lines read `old`, which `edit` moved onto `new`'s key, or `new`.
    fn differ(old: &Self, new: &Self, edit: Splice, dirty: &mut Vec<Range<ByteOffset>>) {
        dirty.push(old.moved_reach(edit));
        dirty.push(new.reach());
    }
}

/// An edit strictly inside `range` grows it; one at either end leaves it out, as `map_chunk` in
/// `inkra-editor` does.
fn mapped(
    range: &Range<ByteOffset>,
    map: impl Fn(ByteOffset, Bias) -> ByteOffset,
) -> Range<ByteOffset> {
    map(range.start, Bias::Right)..map(range.end, Bias::Left)
}

impl Item for Span {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool {
        mapped(&self.range, map) == new.range && self.styles == new.styles
    }
}

impl Item for Conceal {
    fn extent(&self) -> Range<ByteOffset> {
        self.element.clone()
    }

    fn key(&self) -> ByteOffset {
        self.marker.start
    }

    /// Only the marker's line hides or replaces anything.
    fn reach(&self) -> Range<ByteOffset> {
        self.marker.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool {
        mapped(&self.element, &map) == new.element
            && mapped(&self.marker, map) == new.marker
            && self.replacement == new.replacement
    }
}

impl Item for Gutter {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool {
        mapped(&self.range, &map) == new.range
            && self.depth == new.depth
            && self.quotes.len() == new.quotes.len()
            && self
                .quotes
                .iter()
                .zip(&new.quotes)
                .all(|(&old, &new)| map(old, Bias::Right) == new)
            && match (&self.task, &new.task) {
                (Some(old), Some(new)) => {
                    mapped(&(old.start..old.run), &map) == (new.start..new.run)
                        && mapped(&old.checkbox, &map) == new.checkbox
                        && old.checked == new.checked
                }
                (old, new) => old.is_none() && new.is_none(),
            }
    }
}

impl Item for Heading {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool {
        mapped(&self.range, &map) == new.range
            && self.level == new.level
            && self.marker.as_ref().map(|marker| mapped(marker, &map)) == new.marker
    }
}

/// `shape` changes on every edit inside the block, and the layout never reads it.
impl Item for CodeBlock {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    /// Only the opening line reads the block's start and language; any other line reads whether
    /// the block holds it, which changes only between the two ends.
    fn differ(old: &Self, new: &Self, edit: Splice, dirty: &mut Vec<Range<ByteOffset>>) {
        let (was, is) = (edit.map(old.range.end, Bias::Right), new.range.end);
        dirty.push(new.range.start..new.range.start);
        dirty.push(was.min(is)..was.max(is));
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset, Bias) -> ByteOffset) -> bool {
        mapped(&self.range, &map) == new.range
            && self
                .language
                .as_ref()
                .map(|language| mapped(language, &map))
                == new.language
    }
}

#[cfg(test)]
mod tests;
