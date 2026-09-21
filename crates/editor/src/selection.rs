//! Where the caret is, what it has selected, and the column it remembers.
//!
//! `set_head` and `collapse_to` take a `Goal` so no caret move can silently
//! inherit a stale column: only a vertical move carries a real one, and every
//! other caller has to say `Goal::None` out loud.

use std::ops::Range;

use inkra_text::ByteOffset;

/// The remembered horizontal position that makes a run of up or down arrows
/// track a ragged right edge instead of collapsing onto the shortest line.
///
/// Opaque on purpose. It is a pixel x, and only the layer with font metrics can
/// produce or resolve one. What this crate owes it is the discipline of
/// *dropping* it, which is why [`Selection::set_head`] and
/// [`Selection::collapse_to`] take one: there is no way to move the caret
/// without saying what happens to the goal, so no caller can forget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    None,
    Column(f32),
}

/// A cursor, or a selected region. The range is always normalized so
/// `start <= end`; `reversed` records which end the cursor sits on, which is
/// what the platform input protocols ask for and what shift-arrow needs in
/// order to extend the right side.
///
/// Not `Eq`: [`Goal`] holds a float. Two selections over the same range differ
/// only in where the *next* vertical move will land, which is a real difference
/// and one worth seeing in a failing assertion.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    range: Range<ByteOffset>,
    reversed: bool,
    goal: Goal,
}

impl Selection {
    #[must_use]
    pub fn at(offset: ByteOffset) -> Self {
        Self {
            range: offset..offset,
            reversed: false,
            goal: Goal::None,
        }
    }

    #[must_use]
    pub fn goal(&self) -> Goal {
        self.goal
    }

    /// # Panics
    /// If `range` is inverted. Callers build ranges from buffer queries that
    /// are ordered by construction; an inverted one is a bug, not input.
    #[must_use]
    pub fn new(range: Range<ByteOffset>, reversed: bool) -> Self {
        assert!(
            range.start <= range.end,
            "selection range {}..{} is inverted",
            range.start,
            range.end
        );
        Self {
            range,
            reversed,
            goal: Goal::None,
        }
    }

    #[must_use]
    pub fn range(&self) -> Range<ByteOffset> {
        self.range.clone()
    }

    #[must_use]
    pub fn start(&self) -> ByteOffset {
        self.range.start
    }

    #[must_use]
    pub fn end(&self) -> ByteOffset {
        self.range.end
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.range.start == self.range.end
    }

    #[must_use]
    pub fn reversed(&self) -> bool {
        self.reversed
    }

    /// The moving end — where the cursor is drawn and where the next
    /// shift-arrow extends from.
    #[must_use]
    pub fn head(&self) -> ByteOffset {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    /// The anchored end, opposite [`Selection::head`].
    #[must_use]
    pub fn tail(&self) -> ByteOffset {
        if self.reversed {
            self.range.end
        } else {
            self.range.start
        }
    }

    /// Moves the head, keeping the tail anchored, and re-normalizes. Dragging
    /// the head past the tail flips `reversed` rather than inverting the range.
    ///
    /// `goal` is required rather than preserved: a horizontal move must drop it
    /// and a vertical one must keep it, and an argument makes the caller say
    /// which instead of inheriting whatever was there.
    pub fn set_head(&mut self, offset: ByteOffset, goal: Goal) {
        let tail = self.tail();
        if offset < tail {
            self.range = offset..tail;
            self.reversed = true;
        } else {
            self.range = tail..offset;
            self.reversed = false;
        }
        self.goal = goal;
    }

    pub fn collapse_to(&mut self, offset: ByteOffset, goal: Goal) {
        self.range = offset..offset;
        self.reversed = false;
        self.goal = goal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: usize) -> ByteOffset {
        ByteOffset(offset)
    }

    #[test]
    fn a_fresh_selection_is_an_empty_forward_cursor() {
        let selection = Selection::at(at(3));
        assert!(selection.is_empty());
        assert!(!selection.reversed());
        assert_eq!(selection.head(), at(3));
        assert_eq!(selection.tail(), at(3));
    }

    #[test]
    fn extending_forward_moves_the_end_and_anchors_the_start() {
        let mut selection = Selection::at(at(1));
        selection.set_head(at(4), Goal::None);
        assert_eq!(selection.range(), at(1)..at(4));
        assert!(!selection.reversed());
        assert_eq!(selection.head(), at(4));
        assert_eq!(selection.tail(), at(1));
    }

    #[test]
    fn extending_backward_flips_reversed_without_inverting_the_range() {
        let mut selection = Selection::at(at(4));
        selection.set_head(at(1), Goal::None);
        assert_eq!(selection.range(), at(1)..at(4));
        assert!(selection.reversed());
        assert_eq!(selection.head(), at(1));
    }

    #[test]
    fn dragging_the_head_across_the_tail_flips_direction_once() {
        let mut selection = Selection::at(at(2));
        selection.set_head(at(5), Goal::None);
        assert!(!selection.reversed());

        // Past the anchor in the other direction: the tail stays at 2.
        selection.set_head(at(0), Goal::None);
        assert_eq!(selection.range(), at(0)..at(2));
        assert!(selection.reversed());
        assert_eq!(selection.tail(), at(2));
    }

    #[test]
    fn collapsing_clears_the_reversed_flag() {
        let mut selection = Selection::new(at(1)..at(4), true);
        selection.collapse_to(at(4), Goal::None);
        assert!(selection.is_empty());
        assert!(!selection.reversed());
    }

    #[test]
    #[should_panic(expected = "is inverted")]
    fn an_inverted_range_is_refused() {
        let _ = Selection::new(at(4)..at(1), false);
    }
}
