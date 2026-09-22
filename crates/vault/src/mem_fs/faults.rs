use std::path::PathBuf;

use futures::channel::oneshot;

use super::MemFs;

impl MemFs {
    pub fn make_unreadable(&self, path: impl Into<PathBuf>) {
        self.state().unreadable = Some(path.into());
    }

    /// Every `save` of `path` fails from now on. Sticky rather than one-shot:
    /// the conditions it models — a read-only file, a full disk — do not clear
    /// themselves, and a note whose write failed stays dirty, so a one-shot
    /// injector would be repaired by the very next autosave.
    ///
    /// Without it nothing can fail a write, and `SaveFailed` can be deleted
    /// from the write path with the whole suite still green.
    pub fn make_unwritable(&self, path: impl Into<PathBuf>) {
        self.state().unwritable.insert(path.into());
    }

    /// Every `trash` of `path` fails from now on.
    ///
    /// Without it nothing can fail a delete: removing the file behind the
    /// store's back does not, because the write the delete preempts recreates
    /// it first. `TrashFailed` and the whole keep-the-note-on-failure branch
    /// would be unreachable from any test.
    pub fn make_untrashable(&self, path: impl Into<PathBuf>) {
        self.state().untrashable.insert(path.into());
    }

    /// Every `create` or `rename` landing on `path` fails from now on.
    ///
    /// Without it nothing can fail either one, and `RenameFailed` can be
    /// deleted from the rename path with the whole suite still green.
    pub fn make_uncreatable(&self, path: impl Into<PathBuf>) {
        self.state().uncreatable.insert(path.into());
    }

    /// Parks the next `load` of `path` until the returned gate is released or
    /// dropped. One gate parks one load.
    ///
    /// The one piece of latency this fake models: a `BTreeMap` resolves
    /// instantly, so without it no test can hold one note's load open while
    /// another completes — the only shape in which the editor and
    /// `NoteStore::current` can disagree.
    // No `#[must_use]`: `Gate` carries its own, and dropping the returned gate
    // on the spot would release the operation immediately.
    pub fn block_load(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_loads.insert(path.into(), wait);
        Gate(release)
    }

    /// The same, for the next `save` of `path`.
    ///
    /// A write that outlives the turn that issued it is the only way to reach
    /// two writers of one note: the periodic chain's write still in flight when
    /// a second one is issued from outside the chain — `⌘S` queued behind it,
    /// or the quit flush, which cannot await the chain at all. Without this
    /// gate every `MemFs` write completes inside the call that started it, so
    /// that interleaving never occurs and neither the ordering guard nor the
    /// cancel path can be put under test.
    // No `#[must_use]`: see `block_load`.
    pub fn block_save(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_saves.insert(path.into(), wait);
        Gate(release)
    }
}

/// Holds one `MemFs::load` or `MemFs::save` parked. Releasing or dropping it
/// lets that operation finish.
#[must_use]
pub struct Gate(oneshot::Sender<()>);

impl Gate {
    /// Lets the parked operation finish. Dropping the gate does the same; use
    /// this name when the moment it lands is the point of the test.
    pub fn release(self) {
        // Errs when the operation was cancelled before it ever parked — which
        // is exactly what a superseded click is supposed to do to a load.
        let _ = self.0.send(());
    }
}
