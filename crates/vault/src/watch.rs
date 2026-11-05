//! What a watch's events ask a rescan to read (watcher spec, F3 and F4).

use std::collections::BTreeSet;
use std::path::{Component, PathBuf};

use futures::channel::mpsc::UnboundedReceiver;

use crate::fs::Watcher;
use crate::temp::is_temp;
use crate::{CONFIG_DIR, RelPath};

/// A guess, to be measured (watcher spec, F4); Zed has no cap.
const MAX_FOLDERS: usize = 256;

/// A path the watch saw change, with no kind: the consumer stats it again (watcher spec, F1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEvent {
    pub path: PathBuf,
    /// The platform lost events under `path`.
    pub rescan: bool,
}

/// A struct, not a tuple: two roots of one type are easy to swap.
pub struct Watch {
    pub events: UnboundedReceiver<PathEvent>,
    /// As configured; events name it or `resolved` (watcher spec, F3).
    pub root: PathBuf,
    /// `root`, canonical.
    pub resolved: PathBuf,
    pub watcher: Watcher,
}

/// An event that counts, relative to the vault root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changed {
    Path(RelPath),
    /// A path under the vault's `CONFIG_DIR`; it needs no scan.
    Config(RelPath),
    /// The platform lost events: only a full scan knows what changed.
    Rescan,
}

/// The folders the next scan reads, not recursively, or the whole vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Folders(BTreeSet<RelPath>),
    Everything,
}

impl Default for Pending {
    fn default() -> Self {
        Self::Folders(BTreeSet::new())
    }
}

impl Watch {
    /// `None` for an event that counts for nothing: outside the root, hidden but not under
    /// `CONFIG_DIR`, or our own temporary file.
    #[must_use]
    pub fn changed(&self, event: &PathEvent) -> Option<Changed> {
        let path = event.path.as_path();
        let relative = path
            .strip_prefix(&self.root)
            .or_else(|_| path.strip_prefix(&self.resolved))
            .ok()?;
        if relative.file_name().is_some_and(is_temp) {
            return None;
        }
        if matches!(relative.components().next(), Some(Component::Normal(name)) if name == CONFIG_DIR)
        {
            return RelPath::new(relative).map(Changed::Config);
        }
        // As `read_dir`, which lists no dotfile.
        let hidden = relative.components().any(|component| {
            matches!(component, Component::Normal(name) if name.as_encoded_bytes().starts_with(b"."))
        });
        if hidden {
            return None;
        }
        if event.rescan {
            return Some(Changed::Rescan);
        }
        RelPath::new(relative).map(Changed::Path)
    }
}

impl Pending {
    /// A path's folder: reading it finds a folder that appeared at the path (watcher spec, F4).
    pub fn add(&mut self, changed: Changed) {
        match (&mut *self, changed) {
            (Self::Everything, _) | (_, Changed::Config(_)) => {}
            (Self::Folders(folders), Changed::Path(path)) if path != RelPath::root() => {
                folders.extend(path.as_path().parent().and_then(RelPath::new));
                self.cap();
            }
            // The root has no parent to read: only a full scan finds what changed there.
            (Self::Folders(_), Changed::Path(_) | Changed::Rescan) => *self = Self::Everything,
        }
    }

    pub fn merge(&mut self, other: Self) {
        match (&mut *self, other) {
            (Self::Folders(folders), Self::Folders(other)) => {
                folders.extend(other);
                self.cap();
            }
            (Self::Folders(_), Self::Everything) => *self = Self::Everything,
            (Self::Everything, _) => {}
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Folders(folders) if folders.is_empty())
    }

    fn cap(&mut self) {
        if matches!(self, Self::Folders(folders) if folders.len() > MAX_FOLDERS) {
            *self = Self::Everything;
        }
    }
}

#[cfg(test)]
mod tests;
