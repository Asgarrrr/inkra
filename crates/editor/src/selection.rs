use std::ops::Range;

use inkra_text::ByteOffset;

/// Opaque: only the font-metrics layer can produce or resolve one; this crate only carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    None,
    Column(f32),
}

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
    /// If `range` is inverted.
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

    #[must_use]
    pub fn head(&self) -> ByteOffset {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    #[must_use]
    pub fn tail(&self) -> ByteOffset {
        if self.reversed {
            self.range.end
        } else {
            self.range.start
        }
    }

    /// `goal` is required, not preserved, so callers must state it explicitly.
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
mod tests;
