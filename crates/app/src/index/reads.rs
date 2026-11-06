//! `⌘⇧F`'s reads: each candidate read from disk now, at most `READERS` at once, in order.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::{Stream, StreamExt as _};
use gpui::{App, BackgroundExecutor};
use inkra_index::{Candidate, Read};
use inkra_vault::{Fs, VaultError};

use super::IndexSync;

/// Reads in flight per search.
pub(crate) const READERS: usize = 4;

/// Reads candidates through the vault's `Fs`, off the foreground.
#[derive(Clone)]
pub(crate) struct Reader {
    fs: Arc<dyn Fs>,
    root: PathBuf,
    executor: BackgroundExecutor,
}

impl Reader {
    /// Each candidate with its read, in the order given, at most `READERS` in flight.
    pub(crate) fn stream(
        &self,
        candidates: Vec<Candidate>,
    ) -> impl Stream<Item = (Candidate, Read)> + Send + 'static {
        let reader = self.clone();
        futures::stream::iter(candidates)
            .map(move |candidate| {
                let fs = Arc::clone(&reader.fs);
                let path = reader.root.join(&*candidate.path);
                reader.executor.spawn(async move {
                    let read = read(&*fs, &path).await;
                    (candidate, read)
                })
            })
            .buffered(READERS)
    }
}

/// Through `Fs::load`, so the text's line endings fold as the load the words came from did.
async fn read(fs: &dyn Fs, path: &Path) -> Read {
    match fs.load(path).await {
        Ok(loaded) => Read::Text {
            text: loaded.text.into(),
            stamp: loaded.stamp,
        },
        Err(VaultError::NotUtf8 { .. }) => Read::NotUtf8,
        Err(error) if error.io_kind() == Some(ErrorKind::NotFound) => Read::Gone,
        Err(_) => Read::Failed,
    }
}

impl IndexSync {
    /// A reader of the vault this index follows.
    pub(crate) fn reader(&self, cx: &App) -> Reader {
        Reader {
            fs: Arc::clone(&self.fs),
            root: self.vault.read(cx).root().to_owned(),
            executor: cx.background_executor().clone(),
        }
    }
}
