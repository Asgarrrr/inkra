//! The scrub: the seeded notes the index samples, re-read and re-parsed off the foreground; a
//! mismatch rejects the cache through Rebuild (persisted index spec, P16).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Context, Priority};
use inkra_index::persist::ScrubJob;

use super::IndexSync;
use super::loading::load_note;

impl IndexSync {
    pub(super) fn scrub(&self, job: ScrubJob, cx: &mut Context<Self>) {
        let fs = Arc::clone(&self.fs);
        let paths: Vec<PathBuf> = job.paths().map(Path::to_path_buf).collect();
        // Low, as the loads: the scrub must not delay a foreground read.
        let checking = cx
            .background_executor()
            .spawn_with_priority(Priority::Low, async move {
                let mut loaded = Vec::with_capacity(paths.len());
                for path in &paths {
                    loaded.push(load_note(&*fs, path).await);
                }
                job.mismatch(loaded)
            });
        // Detached, holding a weak handle: a verdict landing after this entity is gone is dropped.
        cx.spawn(async move |this, cx| {
            if checking.await {
                log::warn!("index cache rejected: a cached note differs from its file");
                let _ = this.update(cx, Self::rebuild);
            }
        })
        .detach();
    }
}
