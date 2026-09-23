use std::path::PathBuf;
use std::sync::Arc;

use inkra_vault::{Fs, Saved, Stamp};

/// Two writers exist momentarily (the periodic chain and the quit flush);
/// the sequence orders them, since the lock has no fairness guarantee.
pub(super) struct Writer {
    /// No foreground code may hold this guard, or shutdown spins until its timeout.
    state: futures::lock::Mutex<WriterState>,
}

impl Writer {
    pub(super) fn new(stamp: Stamp) -> Self {
        Self {
            state: futures::lock::Mutex::new(WriterState {
                // Below every minted sequence, so the first write is never
                // mistaken for a stale one.
                written: 0,
                stamp,
            }),
        }
    }
}

/// Lives here, not beside `saved_version` on `OpenNote`: a second write
/// issued before the first reaches the lock would otherwise preserve its
/// own new text into a conflict file.
struct WriterState {
    written: u64,
    stamp: Stamp,
}

/// `None` means overtaken, not failed — the caller must not report it and
/// must not advance `saved_version`.
pub(super) async fn write_through(
    writer: Arc<Writer>,
    fs: Arc<dyn Fs>,
    path: PathBuf,
    text: String,
    seq: u64,
) -> Option<inkra_vault::Result<Saved>> {
    let mut state = writer.state.lock().await;
    if seq < state.written {
        return None;
    }
    let result = fs.save(&path, &text, state.stamp).await;
    // Only a landed write advances the mark, or a failure corrupts the
    // next conflict check.
    if let Ok(saved) = &result {
        state.written = seq;
        state.stamp = saved.stamp;
    }
    Some(result)
}

#[cfg(test)]
mod tests;
