//! The index cache file's writer: due by `Writes`, ordered by seq, encoded off the foreground
//! (persisted index spec, P14).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::lock::Mutex;
use gpui::{AppContext as _, Context, Task};
use inkra_index::persist::{Due, Writes, encode};
use inkra_vault::Fs;

use super::IndexSync;

/// Modelled on `SessionWriter`, not shared with it: the encode runs in the write's task (P14).
pub(super) struct CacheWriter {
    fs: Arc<dyn Fs>,
    file: PathBuf,
    root: PathBuf,
    /// The last write that landed; its guard is never held on the foreground.
    written: Arc<Mutex<u64>>,
    next_seq: u64,
    /// `None` until the startup's end.
    pub(super) writes: Option<Writes>,
    /// The timer that asks `Writes` again, and the instant it fires at.
    armed: Option<(Instant, Task<()>)>,
}

impl CacheWriter {
    pub(super) fn new(fs: Arc<dyn Fs>, file: PathBuf, root: PathBuf) -> Self {
        Self {
            fs,
            file,
            root,
            written: Arc::default(),
            next_seq: 0,
            writes: None,
            armed: None,
        }
    }
}

impl IndexSync {
    /// Starts the write `Writes` asks for, or arms the timer that asks again.
    pub(super) fn write_if_due(&mut self, cx: &mut Context<Self>) {
        let Some(writer) = &mut self.writer else {
            return;
        };
        let Some(rule) = &mut writer.writes else {
            return;
        };
        let now = cx.background_executor().now();
        let revision = self.live.revision();
        match rule.due(revision, self.live.status().is_ready(), now) {
            Due::Now => {
                rule.started(revision);
                writer.armed = None;
                let seq = writer.next_seq;
                writer.next_seq += 1;
                let snapshot = self.live.index().snapshot();
                let root = writer.root.clone();
                cx.background_spawn(write_in_order(
                    Arc::clone(&writer.written),
                    Arc::clone(&writer.fs),
                    writer.file.clone(),
                    seq,
                    move || encode(&snapshot, &root),
                ))
                .detach();
            }
            Due::At(at) => {
                if writer.armed.as_ref().is_some_and(|(armed, _)| *armed == at) {
                    return;
                }
                let timer = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(at.saturating_duration_since(now))
                        .await;
                    let _ = this.update(cx, Self::write_if_due);
                });
                writer.armed = Some((at, timer));
            }
            Due::Idle => writer.armed = None,
        }
    }
}

/// Dropping a `Task` stops no write already inside `RealFs`, and the lock has no fairness: the
/// seq orders them. A stale write never encodes, and its snapshot drops off the foreground.
pub(super) async fn write_in_order(
    written: Arc<Mutex<u64>>,
    fs: Arc<dyn Fs>,
    file: PathBuf,
    seq: u64,
    bytes: impl FnOnce() -> Vec<u8>,
) {
    let mut written = written.lock().await;
    if seq < *written {
        return;
    }
    match fs.replace_bytes(&file, &bytes()).await {
        // A failed write leaves the mark, as `write_through`'s.
        Ok(()) => *written = seq,
        Err(error) => log::warn!("index cache not saved: {error}"),
    }
}
