//! The reads of `⌘⇧F` and of the backlinks panel: each candidate from the text cache, else from
//! disk now, at most `READERS` at once. `⌘⇧F` takes them in order, backlinks as each one lands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures::{Stream, StreamExt as _};
use gpui::{App, BackgroundExecutor, Task};
use inkra_index::{Candidate, Read, TextCache};
use inkra_vault::Fs;

use super::IndexSync;

/// Reads in flight per search.
pub(crate) const READERS: usize = 4;

/// Reads candidates through the vault's `Fs`, off the foreground.
#[derive(Clone)]
pub(crate) struct Reader {
    fs: Arc<dyn Fs>,
    root: PathBuf,
    executor: BackgroundExecutor,
    texts: Arc<Mutex<TextCache>>,
}

impl Reader {
    /// Each candidate with its read, in the order given, at most `READERS` in flight.
    pub(crate) fn stream(
        &self,
        candidates: Vec<Candidate>,
    ) -> impl Stream<Item = (Candidate, Read)> + Send + 'static {
        let items = candidates.into_iter().map(|candidate| (candidate, ()));
        self.stream_with(items.collect(), |(), read| read)
    }

    /// As `stream`, each item's `then` run in its reader task, after the read and the cache step.
    pub(crate) fn stream_with<I, T, F>(
        &self,
        items: Vec<(Candidate, I)>,
        then: F,
    ) -> impl Stream<Item = (Candidate, T)> + Send + 'static
    where
        I: Send + 'static,
        T: Send + 'static,
        F: Fn(I, Read) -> T + Clone + Send + 'static,
    {
        self.spawned(items, then).buffered(READERS)
    }

    /// As `stream_with`, each item as its read lands: a held read holds no other.
    pub(crate) fn stream_unordered_with<I, T, F>(
        &self,
        items: Vec<(Candidate, I)>,
        then: F,
    ) -> impl Stream<Item = (Candidate, T)> + Send + 'static
    where
        I: Send + 'static,
        T: Send + 'static,
        F: Fn(I, Read) -> T + Clone + Send + 'static,
    {
        self.spawned(items, then).buffer_unordered(READERS)
    }

    fn spawned<I, T, F>(
        &self,
        items: Vec<(Candidate, I)>,
        then: F,
    ) -> impl Stream<Item = Task<(Candidate, T)>> + Send + 'static
    where
        I: Send + 'static,
        T: Send + 'static,
        F: Fn(I, Read) -> T + Clone + Send + 'static,
    {
        let reader = self.clone();
        futures::stream::iter(items).map(move |(candidate, item)| {
            let fs = Arc::clone(&reader.fs);
            let texts = Arc::clone(&reader.texts);
            let path = reader.root.join(&*candidate.path);
            let then = then.clone();
            reader.executor.spawn(async move {
                let read = read(&*fs, &texts, &candidate, &path).await;
                (candidate, then(item, read))
            })
        })
    }
}

/// A poisoned cache is still whole: each of its operations completes before it unlocks.
pub(super) fn lock(texts: &Mutex<TextCache>) -> MutexGuard<'_, TextCache> {
    texts.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Through `Fs::load`, so the text's line endings fold as the load the words came from did.
async fn read(fs: &dyn Fs, texts: &Mutex<TextCache>, candidate: &Candidate, path: &Path) -> Read {
    if let Some(read) = cached(texts, candidate) {
        return read;
    }
    let read = Read::from(fs.load(path).await);
    if let Read::Text { text, stamp } = &read
        && *stamp == candidate.stamp
    {
        lock(texts).insert(candidate.id, Arc::clone(text), *stamp);
    }
    read
}

fn cached(texts: &Mutex<TextCache>, candidate: &Candidate) -> Option<Read> {
    let text = lock(texts).get(candidate.id, candidate.stamp)?;
    Some(Read::Text {
        text,
        stamp: candidate.stamp,
    })
}

impl IndexSync {
    /// `candidate`'s text if the cache holds it at its stamp; never reads the disk.
    pub(crate) fn cached(&self, candidate: &Candidate) -> Option<Read> {
        cached(&self.texts, candidate)
    }

    /// A reader of the vault this index follows; the open notes' texts stay cached.
    pub(crate) fn reader(&self, cx: &App) -> Reader {
        if let Some(store) = self.notes.upgrade() {
            lock(&self.texts).pin_only(&store.read(cx).open_notes());
        }
        Reader {
            fs: Arc::clone(&self.fs),
            root: self.vault.read(cx).root().to_owned(),
            executor: cx.background_executor().clone(),
            texts: Arc::clone(&self.texts),
        }
    }

    /// Empties the text cache under a new budget; readers already out share it.
    #[cfg(test)]
    pub(crate) fn set_text_budget(&mut self, bytes: usize) {
        *lock(&self.texts) = TextCache::new(bytes);
    }
}
