use std::collections::VecDeque;

use crate::anchor::{Bias, transform};
use crate::offset::ByteOffset;

/// How many splices a buffer keeps: a mark further behind gets `None` from `edits_since`.
const CAPACITY: usize = 256;

/// Where a buffer's splice history stands: every splice moves it forward, undo and redo included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditMark(u64);

/// One text mutation: `removed_len` bytes at `at` replaced by `inserted_len` bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splice {
    pub at: ByteOffset,
    pub removed_len: usize,
    pub inserted_len: usize,
}

impl Splice {
    /// Where `offset`, in the text before the splice, lands after it; `bias` picks an end of the
    /// inserted bytes for an offset at `at` or inside the removed ones.
    #[must_use]
    pub fn map(self, offset: ByteOffset, bias: Bias) -> ByteOffset {
        transform(offset, bias, self.at, self.removed_len, self.inserted_len)
    }

    /// `splices`, oldest first, as one from the text before the first to the text after the last;
    /// `None` for none.
    #[must_use]
    pub fn compose(splices: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut splices = splices.into_iter();
        let first = splices.next()?;
        let (mut start, mut end) = (first.at, first.at + first.inserted_len);
        let (mut removed, mut inserted) = (first.removed_len, first.inserted_len);
        for s in splices {
            start = start.min(s.at);
            end = end.max(s.at + s.removed_len) - s.removed_len + s.inserted_len;
            removed += s.removed_len;
            inserted += s.inserted_len;
        }
        // The text grew by `inserted - removed`, so the hull spanned that much less before.
        let inserted_len = end - start;
        Some(Self {
            at: start,
            inserted_len,
            removed_len: inserted_len + removed - inserted,
        })
    }
}

#[derive(Default)]
pub(super) struct SpliceLog {
    mark: EditMark,
    /// The last `CAPACITY` splices, oldest first.
    splices: VecDeque<Splice>,
}

impl SpliceLog {
    pub(super) fn push(&mut self, splice: Splice) {
        if self.splices.len() == CAPACITY {
            self.splices.pop_front();
        }
        self.splices.push_back(splice);
        self.mark.0 += 1;
    }

    pub(super) fn mark(&self) -> EditMark {
        self.mark
    }

    pub(super) fn since(&self, mark: EditMark) -> Option<impl Iterator<Item = Splice> + '_> {
        assert!(mark <= self.mark, "{mark:?} is past {:?}", self.mark);
        let behind = usize::try_from(self.mark.0 - mark.0).ok()?;
        let first = self.splices.len().checked_sub(behind)?;
        Some(self.splices.range(first..).copied())
    }
}
