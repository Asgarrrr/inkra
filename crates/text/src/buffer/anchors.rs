use crate::anchor::{Anchor, AnchorState, Bias, transform};
use crate::offset::ByteOffset;

use super::Buffer;

impl Buffer {
    /// # Panics
    /// If `offset` is past the end or splits a char — a real `assert!`, since no slicing underneath would catch it in release.
    #[must_use]
    pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor {
        assert!(
            offset.get() <= self.text.len(),
            "anchor offset {offset} past end of buffer"
        );
        assert!(
            self.text.is_char_boundary(offset.get()),
            "anchor offset {offset} not a char boundary"
        );
        self.anchors.push(Some(AnchorState { offset, bias }));
        Anchor(self.anchors.len() - 1)
    }

    /// # Panics
    /// If `anchor` has been forgotten, or was created by another buffer.
    #[must_use]
    pub fn resolve(&self, anchor: Anchor) -> ByteOffset {
        let Some(state) = self.anchors.get(anchor.0).and_then(Option::as_ref) else {
            panic!("anchor {} is not live in this buffer", anchor.0);
        };
        state.offset
    }

    /// Drops an anchor; the slot is tombstoned, never reused, so a stale handle can't silently resolve to a new one.
    ///
    /// # Panics
    /// If `anchor` was already forgotten, or was created by another buffer.
    pub fn forget(&mut self, anchor: Anchor) {
        let state = self.anchors.get_mut(anchor.0).and_then(Option::take);
        assert!(
            state.is_some(),
            "anchor {} is not live in this buffer",
            anchor.0
        );
    }

    pub(super) fn shift_anchors(
        &mut self,
        at: ByteOffset,
        removed_len: usize,
        inserted_len: usize,
    ) {
        for state in self.anchors.iter_mut().flatten() {
            state.offset = transform(state.offset, state.bias, at, removed_len, inserted_len);
        }
    }
}
