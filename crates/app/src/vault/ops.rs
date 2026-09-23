use futures::channel::oneshot;
use gpui::Task;

/// Chains instead of ordering by `Writer`'s sequence: a pending write
/// decided before a rename would still carry a higher sequence and
/// recreate the note under its old name.
#[derive(Default)]
pub(super) struct Ops {
    /// Held, never dropped: a dropped `Task` never runs, so a queued
    /// operation would vanish.
    pub(super) chain: Option<Task<()>>,
    pub(super) scheduled: bool,
    /// Without this, `⌘S` during a pending window is absorbed by
    /// `scheduled` and writes nothing.
    pub(super) cancel: Option<oneshot::Sender<()>>,
    /// `flush_all` skips a note with this set, or it recreates the file
    /// the trash is deleting.
    pub(super) trashing: bool,
}
