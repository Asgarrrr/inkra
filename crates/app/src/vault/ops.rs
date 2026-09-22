use futures::channel::oneshot;
use gpui::Task;

/// Orders every filesystem operation on one note, one after another.
///
/// Separate from `OpenNote`, and keyed in `NoteStore` instead, because the note
/// a user renames or deletes is usually one nobody has opened. An `Ops`
/// therefore outlives the note being open, and exists for notes that never are.
///
/// `open_note`'s load does **not** join this chain, so a rename issued while a
/// note is loading still races it: the load captured its path before the move,
/// finds nothing, and the click silently opens nothing. No data is at risk —
/// the rename itself is ordered — and the window is one background read wide.
/// Closing it means routing the load through here too.
///
/// The chain is the ordering mechanism, and the only one that works for a move.
/// `Writer`'s sequence orders two *writes* by the moment each was decided,
/// which is right when they are competing versions of one text and a category
/// error for a rename: a write pending when a rename is issued is decided
/// *later* and so carries the higher sequence, which either lets it recreate
/// the note under its old name or drops the rename as stale. Neither is
/// reachable here, because `NoteStore::write` does not return until its write
/// has landed, so anything appended behind it runs strictly afterwards.
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
    /// Read by `flush_all`, which skips the note. The flush is the one writer
    /// that cannot join this chain — `App::shutdown` polls it while the
    /// foreground session is blocked — so it mints the highest sequence a note
    /// will ever see, and a trash queued behind it would be dropped as stale.
    /// A confirmed delete would then silently not happen. Set synchronously
    /// when the command is issued, so no lock is involved and the rule that no
    /// foreground code holds the `Writer` guard still stands.
    pub(super) trashing: bool,
}
