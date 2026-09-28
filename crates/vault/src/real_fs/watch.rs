use std::fs;
use std::io;
use std::path::Path;

use futures::channel::mpsc;
use notify::{RecursiveMode, Watcher as _};

use super::io_error;
use crate::fs::{Result, VaultError, Watcher};
use crate::watch::{PathEvent, Watch};

pub(super) fn watch(root: &Path) -> Result<Watch> {
    let resolved = fs::canonicalize(root).map_err(|source| io_error(root, source))?;
    let (sender, events) = mpsc::unbounded();
    let given = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event| {
        for event in path_events(&given, event) {
            // The receiver dropped: nobody listens any more.
            let _ = sender.unbounded_send(event);
        }
    })
    .map_err(|source| watch_error(root, source))?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|source| watch_error(root, source))?;
    Ok(Watch {
        events,
        root: root.to_path_buf(),
        resolved,
        watcher: Box::new(watcher),
    })
}

impl Watcher for notify::RecommendedWatcher {}

fn watch_error(root: &Path, source: notify::Error) -> VaultError {
    io_error(root, io::Error::other(source))
}

/// One event per path; a rescan naming no path, or an error, rescans the root.
fn path_events(root: &Path, event: notify::Result<notify::Event>) -> Vec<PathEvent> {
    let named = event
        .ok()
        .filter(|event| !event.need_rescan() || !event.paths.is_empty());
    let Some(event) = named else {
        return vec![PathEvent {
            path: root.to_path_buf(),
            rescan: true,
        }];
    };
    let rescan = event.need_rescan();
    event
        .paths
        .into_iter()
        .map(|path| PathEvent { path, rescan })
        .collect()
}

#[cfg(test)]
mod tests;
