use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use futures::channel::mpsc::{self, UnboundedSender};

use super::{MemFs, State};
use crate::fs::Watcher;
use crate::watch::{PathEvent, Watch};

pub(super) struct Watching {
    root: PathBuf,
    /// Lives only as long as the caller's `Watcher`.
    sender: Weak<UnboundedSender<PathEvent>>,
}

/// Holds the sender: dropping it ends the stream, as dropping a notify watcher does.
struct MemWatcher {
    _sender: Arc<UnboundedSender<PathEvent>>,
}

impl Watcher for MemWatcher {}

impl State {
    /// As `FSEvents`: one event per path a mutation touched, to every watch above it.
    pub(super) fn changed(&mut self, path: &Path) {
        self.send(path, false);
    }

    fn send(&mut self, path: &Path, rescan: bool) {
        self.watches.retain(|watching| {
            let Some(sender) = watching.sender.upgrade() else {
                return false;
            };
            if path.starts_with(&watching.root) {
                let _ = sender.unbounded_send(PathEvent {
                    path: path.to_path_buf(),
                    rescan,
                });
            }
            true
        });
    }
}

impl MemFs {
    /// What the platform sends when it lost events under `path`.
    pub fn send_rescan(&self, path: impl Into<PathBuf>) {
        self.state().send(&path.into(), true);
    }

    /// No symlink resolves here: `resolved` is `root`.
    pub(super) fn start_watch(&self, root: &Path) -> Watch {
        let (sender, events) = mpsc::unbounded();
        let sender = Arc::new(sender);
        self.state().watches.push(Watching {
            root: root.to_path_buf(),
            sender: Arc::downgrade(&sender),
        });
        Watch {
            events,
            root: root.to_path_buf(),
            resolved: root.to_path_buf(),
            watcher: Box::new(MemWatcher { _sender: sender }),
        }
    }
}
