//! Reading a vault's folders from disk (open-first spec, O3).

use std::collections::HashSet;
use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use super::{MAX_DEPTH, Vault};
use crate::RelPath;
use crate::fs::{DirEntry, Fs, Kind, Result, VaultError};

/// What one scan read from disk.
#[derive(Debug, Default, Clone)]
pub struct Walk {
    /// The folders whose listing it got, whole.
    pub read: Vec<RelPath>,
    /// Their children, absolute.
    pub found: Vec<DirEntry>,
}

pub type Job<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Runs a walk's jobs to completion: the app on its background threads, a test inline.
pub trait Jobs: Sync {
    /// How many jobs a walk starts.
    fn parallelism(&self) -> usize;
    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a>;
}

/// One job, run inline: `scan`, `rescan` and tests.
pub struct Serial;

impl Jobs for Serial {
    fn parallelism(&self) -> usize {
        1
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            for job in jobs {
                job.await;
            }
        })
    }
}

struct Queue {
    folders: Vec<(PathBuf, usize)>,
    /// Jobs reading a folder now: a job finding the queue empty waits for them.
    reading: usize,
    waiting: Vec<Waker>,
    walk: Walk,
    failed: Option<VaultError>,
}

struct Shared<'a> {
    fs: &'a dyn Fs,
    root: &'a Path,
    /// The folder whose failure fails the walk; a seeded walk has none.
    start: Option<&'a Path>,
    queue: Mutex<Queue>,
}

impl Shared<'_> {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The next folder to read, or `None` once no folder is queued and none is being read.
    fn next(&self) -> impl Future<Output = Option<(PathBuf, usize, Reading<'_, '_>)>> + '_ {
        poll_fn(move |task| {
            let mut queue = self.queue();
            if let Some((dir, depth)) = queue.folders.pop() {
                queue.reading += 1;
                return Poll::Ready(Some((dir, depth, Reading(self))));
            }
            if queue.reading == 0 {
                return Poll::Ready(None);
            }
            queue.waiting.push(task.waker().clone());
            Poll::Pending
        })
    }

    async fn job(&self) {
        while let Some((dir, depth, _reading)) = self.next().await {
            let listed = if depth > MAX_DEPTH {
                None
            } else {
                Some(self.fs.read_dir(&dir).await)
            };
            let mut queue = self.queue();
            match listed {
                Some(Ok(children)) => {
                    if let Some(read) = dir.strip_prefix(self.root).ok().and_then(RelPath::new) {
                        queue.walk.read.push(read);
                        for child in children {
                            if child.kind == Kind::Dir {
                                queue.folders.push((child.path.clone(), depth + 1));
                            }
                            queue.walk.found.push(child);
                        }
                    }
                }
                Some(Err(error)) if self.start == Some(dir.as_path()) => {
                    queue.failed = Some(error);
                }
                // Left out of `read`, so `applied` keeps what the vault knew under it.
                Some(Err(_)) | None => {}
            }
            // Before `_reading` drops, which locks the queue again.
            drop(queue);
        }
    }
}

/// A folder counted in `reading` until dropped, so a job that panics still wakes the others.
struct Reading<'s, 'a>(&'s Shared<'a>);

impl Drop for Reading<'_, '_> {
    fn drop(&mut self) {
        let mut queue = self.0.queue();
        queue.reading -= 1;
        let waiting = std::mem::take(&mut queue.waiting);
        drop(queue);
        for waker in waiting {
            waker.wake();
        }
    }
}

impl Vault {
    /// Reads `from` and every folder below it, depth-limited (open-first spec, O3 and O4).
    ///
    /// # Errors
    ///
    /// When `from` itself cannot be read.
    pub async fn walk(fs: &dyn Fs, root: &Path, from: &RelPath, jobs: &dyn Jobs) -> Result<Walk> {
        let start = root.join(from.as_path());
        let folders = vec![(start.clone(), from.as_path().components().count())];
        let queue = run(fs, root, Some(&start), folders, Walk::default(), jobs).await;
        match queue.failed {
            Some(error) => Err(error),
            None => Ok(queue.walk),
        }
    }

    /// Reads every folder `seed` found and did not read, and below them (open-first spec, O3).
    pub async fn walk_seeded(fs: &dyn Fs, root: &Path, seed: Walk, jobs: &dyn Jobs) -> Walk {
        let folders = {
            let read: HashSet<&Path> = seed.read.iter().map(RelPath::as_path).collect();
            seed.found
                .iter()
                .filter(|child| child.kind == Kind::Dir)
                .filter_map(|child| {
                    let path = child.path.strip_prefix(root).ok()?;
                    let depth = path.components().count();
                    (!read.contains(path)).then(|| (child.path.clone(), depth))
                })
                .collect()
        };
        run(fs, root, None, folders, seed, jobs).await.walk
    }
}

async fn run(
    fs: &dyn Fs,
    root: &Path,
    start: Option<&Path>,
    folders: Vec<(PathBuf, usize)>,
    walk: Walk,
    jobs: &dyn Jobs,
) -> Queue {
    let shared = Shared {
        fs,
        root,
        start,
        queue: Mutex::new(Queue {
            folders,
            reading: 0,
            waiting: Vec::new(),
            walk,
            failed: None,
        }),
    };
    let running: Vec<Job<'_>> = (0..jobs.parallelism().max(1))
        .map(|_| Box::pin(shared.job()) as Job<'_>)
        .collect();
    jobs.run(running).await;
    shared
        .queue
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
}
