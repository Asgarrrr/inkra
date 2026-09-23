use std::ops::Range;

use inkra_text::ByteOffset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conceal {
    /// What reveals it: the whole element.
    pub element: Range<ByteOffset>,
    /// What it hides; inside `element` and inside one line.
    pub marker: Range<ByteOffset>,
    /// `""` hides the marker; anything else replaces it.
    pub replacement: &'static str,
}

/// A conceal is active — hidden — when its element does not touch the selection, ends included (D4).
#[must_use]
pub fn is_active(conceal: &Conceal, selection: &Range<ByteOffset>) -> bool {
    !(conceal.element.start <= selection.end && selection.start <= conceal.element.end)
}

/// Elements the selection touches, in order; two selections reveal the same text iff these are equal.
pub fn touched<'a>(
    conceals: &'a [Conceal],
    selection: &'a Range<ByteOffset>,
) -> impl Iterator<Item = &'a Range<ByteOffset>> {
    conceals
        .iter()
        .filter(|conceal| !is_active(conceal, selection))
        .map(|conceal| &conceal.element)
}

#[cfg(test)]
mod tests;
