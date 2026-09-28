//! Following the disk: the watch's events, batched, become scoped rescans (watcher spec, F2).

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use gpui::{Context, Task};
use inkra_vault::{Fs, Pending};

use super::NoteStore;

/// Zed's `FS_WATCH_LATENCY`.
#[cfg(not(test))]
const WATCH_LATENCY: Duration = Duration::from_millis(100);
/// As Zed's fake: a test's events are a batch once it parks.
#[cfg(test)]
const WATCH_LATENCY: Duration = Duration::ZERO;

impl NoteStore {
    /// Held by the store: dropping the task drops the watch. `None` if the platform refuses it.
    pub(super) fn watch(fs: &dyn Fs, root: &Path, cx: &mut Context<Self>) -> Option<Task<()>> {
        let mut watch = match fs.watch(root) {
            Ok(watch) => watch,
            Err(error) => {
                eprintln!("vault watch: {error}");
                return None;
            }
        };
        Some(cx.spawn(async move |this, cx| {
            while let Some(first) = watch.events.next().await {
                cx.background_executor().timer(WATCH_LATENCY).await;
                let mut batch = Pending::default();
                let mut next = Some(first);
                while let Some(event) = next {
                    if let Some(changed) = watch.changed(&event) {
                        batch.add(changed);
                    }
                    next = watch.events.try_recv().ok();
                }
                if this
                    .update(cx, |this, cx| this.rescan_changed(batch, cx))
                    .is_err()
                {
                    return;
                }
            }
        }))
    }
}
