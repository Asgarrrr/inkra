use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use futures::channel::oneshot;

use super::errors::io;
use super::{MemFs, State};
use crate::fs::Result;

impl MemFs {
    /// A folder's listing and a file's load fail with `PermissionDenied`.
    pub fn make_unreadable(&self, path: impl Into<PathBuf>) {
        self.state().unreadable = Some(path.into());
    }

    /// Sticky, not one-shot: the conditions it models don't clear themselves.
    pub fn make_unwritable(&self, path: impl Into<PathBuf>) {
        self.state().unwritable.insert(path.into());
    }

    /// The next `count` saves of `path` fail with `kind`, then saves write again.
    pub fn fail_next_saves(&self, path: impl Into<PathBuf>, count: u32, kind: ErrorKind) {
        self.state()
            .failing_saves
            .insert(path.into(), (count, kind));
    }

    /// The next `count` replaces of `path` fail with `kind` and keep its old bytes.
    pub fn fail_next_replaces(&self, path: impl Into<PathBuf>, count: u32, kind: ErrorKind) {
        self.state()
            .failing_replaces
            .insert(path.into(), (count, kind));
    }

    /// The next `count` removes of `path` fail with `kind` and keep the file.
    pub fn fail_next_removes(&self, path: impl Into<PathBuf>, count: u32, kind: ErrorKind) {
        self.state()
            .failing_removes
            .insert(path.into(), (count, kind));
    }

    /// The next `count` saves of `path` that preserve fail right after preserving, with `kind`.
    pub fn fail_next_saves_after_preserve(
        &self,
        path: impl Into<PathBuf>,
        count: u32,
        kind: ErrorKind,
    ) {
        self.state()
            .failing_after_preserve
            .insert(path.into(), (count, kind));
    }

    /// `watch` of `root` fails with `PermissionDenied`.
    pub fn make_unwatchable(&self, root: impl Into<PathBuf>) {
        self.state().unwatchable.insert(root.into());
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

    /// Parks the next `read_bytes` of `path`, before it reads anything.
    pub fn block_read_bytes(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_read_bytes.insert(path.into(), wait);
        Gate(release)
    }

    pub fn block_save(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_saves.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next `replace_bytes` of `path`, before it stores anything.
    pub fn block_replace(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_replaces.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next `create` of `path`, before it makes anything.
    pub fn block_create(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_creates.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next `read_dir` of `path`, before it reads anything.
    pub fn block_read_dir(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_read_dirs.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next `save` of `path` after the write: the file already has its new inode.
    pub fn block_after_save(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_after_saves.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next rename of `from` after the move: the file is already at its new path.
    pub fn block_after_rename(&self, from: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_after_renames.insert(from.into(), wait);
        Gate(release)
    }
}

impl State {
    pub(super) fn refused_load(&self, path: &Path) -> Result<()> {
        if self.unreadable.as_deref() == Some(path) {
            return Err(io(path, ErrorKind::PermissionDenied));
        }
        Ok(())
    }

    pub(super) fn refused_watch(&self, root: &Path) -> Result<()> {
        if self.unwatchable.contains(root) {
            return Err(io(root, ErrorKind::PermissionDenied));
        }
        Ok(())
    }

    pub(super) fn injected_save_failure(&mut self, path: &Path) -> Result<()> {
        injected(&mut self.failing_saves, path)
    }

    pub(super) fn injected_replace_failure(&mut self, path: &Path) -> Result<()> {
        injected(&mut self.failing_replaces, path)
    }

    pub(super) fn injected_remove_failure(&mut self, path: &Path) -> Result<()> {
        injected(&mut self.failing_removes, path)
    }

    pub(super) fn injected_failure_after_preserve(&mut self, path: &Path) -> Result<()> {
        injected(&mut self.failing_after_preserve, path)
    }
}

/// Spends one of `path`'s failures; the last one removes its entry.
fn injected(failing: &mut BTreeMap<PathBuf, (u32, ErrorKind)>, path: &Path) -> Result<()> {
    let Some((left @ 1.., kind)) = failing.get_mut(path) else {
        return Ok(());
    };
    let kind = *kind;
    *left -= 1;
    if *left == 0 {
        failing.remove(path);
    }
    Err(io(path, kind))
}

/// Holds one parked `MemFs` operation. Releasing or dropping it lets that
/// operation finish.
#[must_use]
pub struct Gate(oneshot::Sender<()>);

impl Gate {
    /// Lets the parked operation finish. Dropping the gate does the same.
    pub fn release(self) {
        let _ = self.0.send(());
    }
}
