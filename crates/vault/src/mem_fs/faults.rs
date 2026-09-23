use std::path::PathBuf;

use futures::channel::oneshot;

use super::MemFs;

impl MemFs {
    pub fn make_unreadable(&self, path: impl Into<PathBuf>) {
        self.state().unreadable = Some(path.into());
    }

    /// Sticky, not one-shot: the conditions it models don't clear themselves.
    pub fn make_unwritable(&self, path: impl Into<PathBuf>) {
        self.state().unwritable.insert(path.into());
    }

    pub fn make_untrashable(&self, path: impl Into<PathBuf>) {
        self.state().untrashable.insert(path.into());
    }

    pub fn make_uncreatable(&self, path: impl Into<PathBuf>) {
        self.state().uncreatable.insert(path.into());
    }

    /// One gate parks one load: a second call on the same path before release
    /// silently replaces the first.
    // No `#[must_use]`: `Gate` carries its own.
    pub fn block_load(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_loads.insert(path.into(), wait);
        Gate(release)
    }

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
    /// Lets the parked operation finish. Dropping the gate does the same.
    pub fn release(self) {
        let _ = self.0.send(());
    }
}
