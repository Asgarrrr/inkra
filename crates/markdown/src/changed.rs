//! Which part of a note's layout an edit can change, from the parses before and after it
//! (incremental layout spec, D6, coarse form).

use std::ops::Range;

use inkra_text::{Bias, ByteOffset, Splice};

use crate::{CodeBlock, Conceal, Gutter, Heading, Markdown, Span};

/// The bytes of `new`'s text outside which every line reads the same items as before `edit`, which
/// turned `old`'s text into `new`'s. Each vector splits into a common prefix before the edit, a
/// common suffix after it, and a middle; the region covers the edit and both middles.
#[must_use]
pub fn changed(old: &Markdown, new: &Markdown, edit: Splice) -> Range<ByteOffset> {
    let mut region = edit.at..edit.at + edit.inserted_len;
    middle(&old.spans, &new.spans, edit, &mut region);
    middle(&old.conceals, &new.conceals, edit, &mut region);
    middle(&old.gutters, &new.gutters, edit, &mut region);
    middle(&old.headings, &new.headings, edit, &mut region);
    middle(&old.code_blocks, &new.code_blocks, edit, &mut region);
    region
}

fn middle<T: Item>(old: &[T], new: &[T], edit: Splice, region: &mut Range<ByteOffset>) {
    let removed_end = edit.at + edit.removed_len;
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(old, new)| old.extent().end <= edit.at && old.same(new, |offset| offset))
        .count();
    let shift = |offset: ByteOffset| offset + edit.inserted_len - edit.removed_len;
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(old, new)| old.extent().start >= removed_end && old.same(new, shift))
        .count();
    for item in &new[prefix..new.len() - suffix] {
        grow(region, item.reach());
    }
    for item in &old[prefix..old.len() - suffix] {
        let reach = item.reach();
        grow(
            region,
            edit.map(reach.start, Bias::Left)..edit.map(reach.end, Bias::Right),
        );
    }
}

fn grow(region: &mut Range<ByteOffset>, by: Range<ByteOffset>) {
    *region = region.start.min(by.start)..region.end.max(by.end);
}

/// What the layout reads of one parse item.
trait Item {
    /// Holds every offset of the item.
    fn extent(&self) -> Range<ByteOffset>;

    /// The text whose lines read the item.
    fn reach(&self) -> Range<ByteOffset> {
        self.extent()
    }

    /// Whether the layout reads `self`, each offset moved by `map`, as it reads `new`.
    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool;
}

fn mapped(range: &Range<ByteOffset>, map: impl Fn(ByteOffset) -> ByteOffset) -> Range<ByteOffset> {
    map(range.start)..map(range.end)
}

impl Item for Span {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool {
        mapped(&self.range, map) == new.range && self.styles == new.styles
    }
}

impl Item for Conceal {
    fn extent(&self) -> Range<ByteOffset> {
        self.element.clone()
    }

    /// Only the marker's line hides or replaces anything.
    fn reach(&self) -> Range<ByteOffset> {
        self.marker.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool {
        mapped(&self.element, &map) == new.element
            && mapped(&self.marker, map) == new.marker
            && self.replacement == new.replacement
    }
}

impl Item for Gutter {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool {
        mapped(&self.range, &map) == new.range
            && self.depth == new.depth
            && self.quotes.len() == new.quotes.len()
            && self
                .quotes
                .iter()
                .zip(&new.quotes)
                .all(|(&old, &new)| map(old) == new)
    }
}

impl Item for Heading {
    fn extent(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool {
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

    fn same(&self, new: &Self, map: impl Fn(ByteOffset) -> ByteOffset) -> bool {
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
