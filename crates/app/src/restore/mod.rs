//! The session file, read, planned onto the vault and written in order; no shell state (restore-tabs spec).

mod format;
mod plan;
mod view;

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use futures::lock::Mutex;
use gpui::{App, AppContext as _, Task};
use inkra_vault::Fs;

#[cfg(test)]
pub(crate) use format::{SavedAxis, SavedDock, SavedDocks, SessionV1};
pub(crate) use format::{
    SavedGroup, SavedMember, SavedNoteTab, SavedTab, SavedView, Session, session_file,
};
pub(crate) use plan::{Landed, Pending, Plan, Restored, Surfaces};

/// The session file of a vault launch, read before the first frame.
pub(crate) struct Opened {
    pub(crate) file: PathBuf,
    /// None: missing, unreadable or rejected.
    pub(crate) session: Option<Session>,
}

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

    /// `None` only on a root that is not UTF-8.
    pub(crate) fn write(&mut self, session: &Session, cx: &App) -> Option<Task<()>> {
        let text = session
            .encode()
            .inspect_err(|error| log::warn!("session not saved: {error}"))
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
        Err(error) if error.io_kind() == Some(ErrorKind::NotFound) => return None,
        Err(error) => {
            log::warn!("session not restored: {error}");
            return None;
        }
    };
    Session::decode(&loaded.text, &root)
        .inspect_err(|rejected| log::warn!("session not restored: {}: {rejected}", file.display()))
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
        Err(error) => log::warn!("session not saved: {error}"),
    }
}

#[cfg(test)]
mod tests;
