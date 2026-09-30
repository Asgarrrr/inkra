//! The session file, read, planned onto the vault and written in order; no shell state (restore-tabs spec).

mod format;
mod plan;
mod view;

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use futures::lock::Mutex;
use gpui::{App, AppContext as _, Task};
use inkra_vault::{EntryId, Fs, Vault, VaultError};

pub(crate) use format::{SavedTab, SavedView, Session, session_file};
pub(crate) use plan::{Plan, Restored};

use crate::editor::ViewState;

/// Writes the session file; each write carries the age of its snapshot.
pub(crate) struct SessionWriter {
    fs: Arc<dyn Fs>,
    file: PathBuf,
    /// The last write that landed; its guard is never held on the foreground.
    written: Arc<Mutex<u64>>,
    next_seq: u64,
}

impl SessionWriter {
    pub(crate) fn new(fs: Arc<dyn Fs>, file: PathBuf) -> Self {
        Self {
            fs,
            file,
            written: Arc::default(),
            next_seq: 0,
        }
    }

    /// A tab whose note has no entry or no UTF-8 path saves empty; `None` only on a root that is not UTF-8.
    pub(crate) fn write(
        &mut self,
        vault: &Vault,
        tabs: impl Iterator<Item = (Option<EntryId>, Option<ViewState>)>,
        active: usize,
        cx: &App,
    ) -> Option<Task<()>> {
        let tabs = tabs
            .map(|(note, view)| {
                let note = note
                    .and_then(|id| vault.entry(id))
                    .and_then(|entry| entry.path.as_path().to_str())
                    .map(str::to_owned);
                let view = view
                    .as_ref()
                    .filter(|_| note.is_some())
                    .map(SavedView::from);
                SavedTab { note, view }
            })
            .collect();
        let text = Session::new(vault.root().to_path_buf(), active, tabs)
            .encode()
            .inspect_err(|error| eprintln!("session not saved: {error}"))
            .ok()?;
        self.next_seq += 1;
        Some(cx.background_spawn(replace_in_order(
            Arc::clone(&self.written),
            Arc::clone(&self.fs),
            self.file.clone(),
            text,
            self.next_seq,
        )))
    }
}

/// The session of the vault at `root`: `None`, silent, if there is none yet; any other failure logs one line.
pub(crate) async fn read(fs: Arc<dyn Fs>, file: PathBuf, root: PathBuf) -> Option<Session> {
    let loaded = match fs.load(&file).await {
        Ok(loaded) => loaded,
        Err(VaultError::Io { source, .. }) if source.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("session not restored: {error}");
            return None;
        }
    };
    Session::decode(&loaded.text, &root)
        .inspect_err(|rejected| eprintln!("session not restored: {}: {rejected}", file.display()))
        .ok()
}

/// Dropping a `Task` stops no write already inside `RealFs`, and the lock has no fairness: the seq orders them.
async fn replace_in_order(
    written: Arc<Mutex<u64>>,
    fs: Arc<dyn Fs>,
    file: PathBuf,
    text: String,
    seq: u64,
) {
    let mut written = written.lock().await;
    if seq < *written {
        return;
    }
    match fs.replace(&file, &text).await {
        // A failed write leaves the mark, as `write_through`'s.
        Ok(()) => *written = seq,
        Err(error) => eprintln!("session not saved: {error}"),
    }
}

#[cfg(test)]
mod tests;
