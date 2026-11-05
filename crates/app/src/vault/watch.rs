//! Following the disk: the watch's events, batched, become scoped rescans (watcher spec, F2).

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use gpui::{Context, Task};
use inkra_vault::{CONFIG_DIR, Changed, Fs, Pending, RelPath, VaultError};

use super::{ConfigChanged, NoteStore};

/// Zed's `FS_WATCH_LATENCY`.
#[cfg(not(test))]
const WATCH_LATENCY: Duration = Duration::from_millis(100);
/// As Zed's fake: a test's events are a batch once it parks.
#[cfg(test)]
const WATCH_LATENCY: Duration = Duration::ZERO;

impl NoteStore {
    /// Held by the store: dropping the task drops the watch. An error if the platform refuses it.
    pub(super) fn watch(
        fs: &dyn Fs,
        root: &Path,
        cx: &mut Context<Self>,
    ) -> Result<Task<()>, VaultError> {
        let mut watch = fs.watch(root)?;
        Ok(cx.spawn(async move |this, cx| {
            while let Some(first) = watch.events.next().await {
                cx.background_executor().timer(WATCH_LATENCY).await;
                let mut batch = Pending::default();
                let mut configs = Vec::new();
                let mut next = Some(first);
                while let Some(event) = next {
                    match watch.changed(&event) {
                        Some(Changed::Config(path)) => configs.push(path),
                        Some(Changed::Rescan) => {
                            // The lost events may include `CONFIG_DIR`'s.
                            configs.extend(RelPath::new(Path::new(CONFIG_DIR)));
                            batch.add(Changed::Rescan);
                        }
                        Some(changed) => batch.add(changed),
                        None => {}
                    }
                    next = watch.events.try_recv().ok();
                }
                if this
                    .update(cx, |this, cx| {
                        if !configs.is_empty() {
                            cx.emit(ConfigChanged(configs));
                        }
                        this.rescan_changed(batch, cx);
                    })
                    .is_err()
                {
                    return;
                }
            }
        }))
    }
}
