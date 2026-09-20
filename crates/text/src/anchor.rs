//! Positions that survive edits.
//!
//! The pure half lives here: [`transform`] moves one offset through one edit
//! and needs no buffer to do it. The table of live anchors is a field on
//! [`crate::Buffer`], which is the only thing that can make or read an
//! [`Anchor`].

use crate::offset::ByteOffset;

/// Which text an anchor stays attached to when an edit lands on its own
/// offset: `Left` the text before the edit, `Right` the text after.
///
/// Same convention as Zed's `anchor_before`/`anchor_after`
/// (`vendor/zed-reference/crates/text/src/anchor.rs`), restricted to the
/// insertion-point meaning — Zed resolves replace boundaries through its
/// fragment model, which this does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}

/// A handle to a position the creating buffer keeps up to date across edits.
///
/// Opaque on purpose: it is an index into that buffer's table, not an offset.
/// Nothing outside `text` can read it, so the representation stays swappable.
///
/// Deliberately not `PartialOrd`/`Ord`. The index is creation order, not
/// document order, and comparing two anchors means resolving both first. Zed
/// makes the same point by requiring a buffer to compare
/// (`vendor/zed-reference/crates/text/src/anchor.rs:91-103`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Anchor(pub(crate) usize);

/// What the buffer stores per live anchor.
pub(crate) struct AnchorState {
    pub(crate) offset: ByteOffset,
    pub(crate) bias: Bias,
}

/// Moves `offset` through one edit: `removed_len` bytes at `at` replaced by
/// `inserted_len` bytes.
///
/// Two boundaries, and only one of them is ambiguous. `at` is the edit's
/// start, the sole boundary a pure insertion has, and `bias` decides there —
/// that is what `Bias` is for. `at + removed_len` is the edit's *exclusive*
/// end: an offset sitting exactly on it was never inside the removed text, so
/// it shifts like anything further right and bias plays no part.
pub(crate) fn transform(
    offset: ByteOffset,
    bias: Bias,
    at: ByteOffset,
    removed_len: usize,
    inserted_len: usize,
) -> ByteOffset {
    let touched_end = at.0 + removed_len;
    if offset.0 < at.0 {
        offset
    } else if offset.0 == at.0 || (removed_len > 0 && offset.0 < touched_end) {
        match bias {
            Bias::Left => at,
            Bias::Right => ByteOffset(at.0 + inserted_len),
        }
    } else {
        // No underflow: this branch only runs where `offset.0 >= touched_end
        // >= removed_len`.
        ByteOffset(offset.0 - removed_len + inserted_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: usize) -> ByteOffset {
        ByteOffset(offset)
    }

    #[test]
    fn an_offset_before_the_edit_does_not_move() {
        for bias in [Bias::Left, Bias::Right] {
            assert_eq!(transform(at(0), bias, at(2), 2, 3), at(0));
            assert_eq!(transform(at(1), bias, at(2), 2, 3), at(1));
        }
    }

    #[test]
    fn an_offset_after_the_edit_shifts_by_the_length_change() {
        for bias in [Bias::Left, Bias::Right] {
            assert_eq!(transform(at(5), bias, at(1), 2, 3), at(6));
            assert_eq!(transform(at(5), bias, at(1), 2, 1), at(4));
        }
    }

    #[test]
    fn a_pure_insertion_at_the_offset_is_decided_by_bias() {
        assert_eq!(transform(at(2), Bias::Left, at(2), 0, 3), at(2));
        assert_eq!(transform(at(2), Bias::Right, at(2), 0, 3), at(5));
    }

    #[test]
    fn an_offset_inside_the_removed_span_collapses_to_its_bias_side() {
        // "abcde", replace [1, 4) — offset 2 is strictly inside "bcd".
        for inserted_len in [1, 5] {
            assert_eq!(transform(at(2), Bias::Left, at(1), 3, inserted_len), at(1));
            assert_eq!(
                transform(at(2), Bias::Right, at(1), 3, inserted_len),
                ByteOffset(1 + inserted_len)
            );
        }
    }

    #[test]
    fn the_start_of_a_non_empty_replace_is_decided_by_bias() {
        assert_eq!(transform(at(1), Bias::Left, at(1), 3, 2), at(1));
        // Right clears the inserted text rather than stopping at `at`.
        assert_eq!(transform(at(1), Bias::Right, at(1), 3, 2), at(3));
    }

    #[test]
    fn the_exclusive_end_of_a_replace_shifts_under_both_biases() {
        // "abcde", replace [1, 4) with "XY" → "aXYe". Offset 4 sat before 'e'
        // and was never inside "bcd", so neither bias may collapse it.
        for bias in [Bias::Left, Bias::Right] {
            assert_eq!(transform(at(4), bias, at(1), 3, 2), at(3));
        }
    }

    #[test]
    fn the_end_of_a_pure_deletion_tracks_onto_its_start() {
        // "abc", delete [0, 1): both branches of the function agree here, and
        // this is the case that round-trips through undo exactly.
        assert_eq!(transform(at(1), Bias::Right, at(0), 1, 0), at(0));
        assert_eq!(transform(at(1), Bias::Left, at(0), 1, 0), at(0));
    }
}
