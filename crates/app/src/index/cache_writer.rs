//! The index cache file's writer: due by `Writes`, ordered by seq, encoded off the foreground
//! (persisted index spec, P14).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::lock::Mutex;
use gpui::{App, AppContext as _, Context, Task};
use inkra_index::persist::{Due, MAX_BYTES, Writes, encode};
use inkra_vault::Fs;

use super::IndexSync;

/// Modelled on `SessionWriter`, not shared with it: the encode runs in the write's task (P14).
pub(super) struct CacheWriter {
    fs: Arc<dyn Fs>,
    file: PathBuf,
    root: PathBuf,
    /// The seq of the last write that landed or removal that ran; never locked on the foreground.
    mark: Arc<Mutex<u64>>,
    next_seq: u64,
    /// `None` until the startup's end.
    writes: Option<Writes>,
    /// The timer that asks `Writes` again, and the instant it fires at.
    armed: Option<(Instant, Task<()>)>,
}

impl CacheWriter {
    pub(super) fn new(fs: Arc<dyn Fs>, file: PathBuf, root: PathBuf) -> Self {
        Self {
            fs,
            file,
            root,
            mark: Arc::default(),
            next_seq: 0,
            writes: None,
            armed: None,
        }
    }

    /// Starts `Writes` at `indexed`: `held` is the revision the file holds (P14).
    pub(super) fn start(&mut self, held: Option<u64>) {
        self.writes = Some(Writes::new(held));
    }

    /// Removes the file in the writes' seq order: no write started before it lands after it (P15).
    pub(super) fn remove(&mut self, cx: &App) {
        let seq = self.take_seq();
        if let Some(writes) = &mut self.writes {
            writes.removed();
        }
        cx.background_spawn(remove_in_order(
            Arc::clone(&self.mark),
            Arc::clone(&self.fs),
            self.file.clone(),
            seq,
        ))
        .detach();
    }

    /// The seq a write or the removal is ordered by.
    fn take_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
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
                let seq = writer.take_seq();
                let snapshot = self.live.index().snapshot();
                let root = writer.root.clone();
                let write = cx.background_spawn(write_in_order(
                    Arc::clone(&writer.mark),
                    Arc::clone(&writer.fs),
                    writer.file.clone(),
                    seq,
                    move || encode(&snapshot, &root),
                ));
                cx.spawn(async move |this, cx| {
                    let written = write.await;
                    let _ = this.update(cx, |sync, cx| sync.written(written, revision, cx));
                })
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

    /// Feeds a write's outcome back to `Writes`: a failure backs off, then asks again.
    fn written(&mut self, written: Written, revision: u64, cx: &mut Context<Self>) {
        let Some(rule) = self
            .writer
            .as_mut()
            .and_then(|writer| writer.writes.as_mut())
        else {
            return;
        };
        match written {
            Written::Landed => rule.wrote(),
            Written::Failed => {
                rule.failed(revision, cx.background_executor().now());
                self.write_if_due(cx);
            }
            Written::Skipped => {}
        }
    }
}

/// How a write ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Written {
    Landed,
    Failed,
    /// A newer write had landed first.
    Skipped,
}

/// Dropping a `Task` stops no write already inside `RealFs`, and the lock has no fairness: the
/// seq orders them. A stale write never encodes, and its snapshot drops off the foreground.
pub(super) async fn write_in_order(
    mark: Arc<Mutex<u64>>,
    fs: Arc<dyn Fs>,
    file: PathBuf,
    seq: u64,
    bytes: impl FnOnce() -> Option<Vec<u8>>,
) -> Written {
    let mut mark = mark.lock().await;
    if seq < *mark {
        return Written::Skipped;
    }
    let Some(bytes) = bytes() else {
        log::warn!("index cache not saved: over the cap of {MAX_BYTES} bytes");
        return Written::Failed;
    };
    match fs.replace_bytes(&file, &bytes).await {
        // A failed write leaves the mark, as `write_through`'s.
        Ok(()) => {
            *mark = seq;
            Written::Landed
        }
        Err(error) => {
            log::warn!("index cache not saved: {error}");
            Written::Failed
        }
    }
}

/// Moves the mark even when the removal fails, so an older write in flight cannot bring the file back.
pub(super) async fn remove_in_order(
    mark: Arc<Mutex<u64>>,
    fs: Arc<dyn Fs>,
    file: PathBuf,
    seq: u64,
) {
    let mut mark = mark.lock().await;
    if seq < *mark {
        return;
    }
    if let Err(error) = fs.remove_file(&file).await {
        log::warn!("index cache not removed: {error}");
    }
    *mark = seq;
}
