use std::collections::VecDeque;

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
