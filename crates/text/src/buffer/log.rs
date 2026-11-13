/// Where a buffer's splice history stands: every splice moves it forward, undo and redo included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditMark(u64);

#[derive(Default)]
pub(super) struct SpliceLog {
    mark: EditMark,
}

impl SpliceLog {
    pub(super) fn push(&mut self) {
        self.mark.0 += 1;
    }

    pub(super) fn mark(&self) -> EditMark {
        self.mark
    }
}
