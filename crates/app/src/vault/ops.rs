use futures::channel::oneshot;
use gpui::Task;

/// Orders every filesystem operation on one note, one after another. Keyed in
/// `NoteStore`, not `OpenNote`: the note being renamed or deleted is usually
/// not open, so an `Ops` outlives the note being open, or is never one.
///
/// `open_note`'s load does **not** join this chain: a rename racing a load
/// can leave a click opening nothing (no data lost, one background-read
/// window). Closing it means routing the load through here too.
///
/// Why the chain, not `Writer`'s sequence: a write pending when a rename is
/// issued is decided later and carries a higher sequence — ordering by it
/// would recreate the note under its old name or drop the rename as stale.
/// Safe here because a chain link awaits `NoteStore::write` to completion,
/// so anything appended behind it runs strictly afterwards. Full reasoning:
/// CLAUDE.md, "Every filesystem operation on one note...".
#[derive(Default)]
pub(super) struct Ops {
    /// The pending chain. Held, and never replaced by dropping it: a dropped
    /// `Task` never runs, so a queued operation would vanish
    /// (`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`). Each
    /// new link awaits this one instead.
    pub(super) chain: Option<Task<()>>,
    /// A write is queued and has not reached its timer yet. Edits check this
    /// and return, so a run of typing keeps the first deadline rather than
    /// pushing it forward for as long as the user keeps going.
    pub(super) scheduled: bool,
    /// Resolves the pending write's timer early. `⌘S`, a note switch and the
    /// quit flush use it; an edit never does. Without it an explicit `⌘S`
    /// during a pending window would be absorbed by `scheduled` and write
    /// nothing, up to `AUTOSAVE_DELAY` late.
    pub(super) cancel: Option<oneshot::Sender<()>>,
    /// A trash is queued and has not run yet.
    ///
    /// Read by `flush_all`, which skips the note: the flush can't join this
    /// chain, so it mints the highest sequence the note will ever see, and a
    /// trash queued behind it would be dropped as stale — silently undoing a
    /// confirmed delete. Set synchronously, so no foreground code ever holds
    /// the `Writer` guard.
    pub(super) trashing: bool,
}
