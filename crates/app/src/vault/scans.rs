//! One scan worker per store: serial scans, coalesced requests, replayed re-keys (open-first spec, O2).

use std::sync::Arc;

use futures::channel::oneshot;
use gpui::{BackgroundExecutor, Context, Entity, Priority, prelude::*};
use inkra_vault::{EntryId, FileIdentity, Job, Jobs, Mtime, RelPath, Result, Vault, Walk};

use super::NoteStore;
use crate::timeline::{Step, Timeline, timed};

#[derive(Default)]
pub(super) struct Scans {
    /// Between a scan's start and its swap.
    running: Option<Running>,
}

#[derive(Default)]
struct Running {
    /// Answered by the next scan, which starts once the running one has swapped.
    waiting: Vec<oneshot::Sender<()>>,
    /// Re-keys since the running scan took its base, replayed at its swap.
    reidentified: Vec<(EntryId, FileIdentity, Mtime)>,
}

/// The initial scan reads the root level first, so the sidebar shows it before the full walk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Root,
    Full,
}

/// The walk's jobs on the background threads, at low priority as Zed's scanner.
struct Background(BackgroundExecutor);

impl Jobs for Background {
    fn parallelism(&self) -> usize {
        self.0.num_cpus()
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(self.0.scoped_priority(Priority::Low, move |scope| {
            for job in jobs {
                scope.spawn(job);
            }
        }))
    }
}

impl NoteStore {
    /// Resolves once a scan that started after this call has swapped in; dropped if its walk fails.
    pub(super) fn rescan(&mut self, cx: &mut Context<Self>) -> oneshot::Receiver<()> {
        let (applied, wait) = oneshot::channel();
        if self.vault.is_some() {
            match &mut self.scans.running {
                Some(running) => running.waiting.push(applied),
                None => self.start_scan(vec![applied], cx),
            }
        }
        wait
    }

    /// After every save; during a scan only recorded, for the swap to replay (open-first spec, O2).
    pub(super) fn reidentify(
        &mut self,
        id: EntryId,
        identity: FileIdentity,
        mtime: Mtime,
        cx: &mut Context<Self>,
    ) {
        if let Some(running) = &mut self.scans.running {
            running.reidentified.push((id, identity, mtime));
            return;
        }
        if let Some(vault) = self.vault.clone() {
            vault.update(cx, |vault, cx| {
                if vault.reidentify(id, identity, mtime) {
                    cx.notify();
                }
            });
        }
    }

    /// `answers` resolve once this scan swaps in.
    pub(super) fn start_scan(&mut self, answers: Vec<oneshot::Sender<()>>, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let everything = RelPath::root();
        self.scans.running = Some(Running::default());
        // Taken in the turn that sets `running`: every re-key from here on is recorded and replayed.
        let base = vault.read(cx).clone();
        let fs = Arc::clone(&self.fs);
        let jobs = Background(cx.background_executor().clone());
        // Detached: a store dropped mid-scan lets the walk finish, then its updates find nothing.
        cx.spawn(async move |this, cx| {
            let root = base.root().to_path_buf();
            let mut base = base;
            // The root pass's listing, which the full pass continues instead of listing the root again.
            let seed = if base.is_scanned() {
                None
            } else {
                let (fs, root) = (Arc::clone(&fs), root.clone());
                let read = vec![everything.clone()];
                let listed = cx
                    .background_spawn(async move {
                        let listing = Walk {
                            read,
                            found: fs.read_dir(&root).await?,
                        };
                        Ok((base.applied(listing.clone()), listing))
                    })
                    .await;
                let (listed, seed) = match listed {
                    Ok((applied, listing)) => (Ok(applied), Some(listing)),
                    Err(error) => (Err(error), None),
                };
                // One turn: no re-key falls between the root pass's swap and the full pass's base.
                let Ok((applied, next)) = timed(cx, |cx| {
                    this.update(cx, |this, cx| {
                        (
                            this.swap(&vault, listed, Pass::Root, cx),
                            vault.read(cx).clone(),
                        )
                    })
                }) else {
                    return;
                };
                base = next;
                if applied {
                    cx.update(|cx| Timeline::mark(Step::Root, cx));
                }
                seed
            };
            let scanned = cx
                .background_spawn(async move {
                    let walk = match seed {
                        Some(seed) => Vault::walk_seeded(&*fs, &root, seed, &jobs).await,
                        None => Vault::walk(&*fs, &root, &everything, &jobs).await?,
                    };
                    Ok(base.applied(walk))
                })
                .await;
            let _ = timed(cx, |cx| {
                this.update(cx, |this, cx| {
                    let applied = this.swap(&vault, scanned, Pass::Full, cx);
                    this.scan_ended(answers, applied, cx);
                })
            });
            cx.update(|cx| {
                if vault.read(cx).is_scanned() {
                    Timeline::mark(Step::Scanned, cx);
                }
            });
        })
        .detach();
    }

    /// Swaps a scan's result in with the re-keys made meanwhile; `false` if the walk failed.
    fn swap(
        &mut self,
        vault: &Entity<Vault>,
        scanned: Result<Vault>,
        pass: Pass,
        cx: &mut Context<Self>,
    ) -> bool {
        let reidentified = self
            .scans
            .running
            .as_mut()
            .map(|running| std::mem::take(&mut running.reidentified))
            .unwrap_or_default();
        let mut next = match scanned {
            Ok(next) => next,
            Err(error) => {
                eprintln!("vault scan: {error}");
                vault.update(cx, |vault, cx| {
                    // The base went with the walk, so the live state is no longer shared.
                    let mut moved = false;
                    for (id, identity, mtime) in reidentified {
                        moved |= vault.reidentify(id, identity, mtime);
                    }
                    // O8: the index builds on what is known, so quick open never waits forever.
                    let scanned = pass == Pass::Full && !vault.is_scanned();
                    if scanned {
                        vault.mark_scanned();
                    }
                    if moved || scanned {
                        cx.notify();
                    }
                });
                return false;
            }
        };
        for (id, identity, mtime) in reidentified {
            let _ = next.reidentify(id, identity, mtime);
        }
        if pass == Pass::Full {
            next.mark_scanned();
        }
        let previous = vault.update(cx, |vault, cx| {
            let previous = std::mem::replace(vault, next);
            cx.notify();
            previous
        });
        // The replaced state may be the last owner of 50,000 entries: freeing them is foreground time.
        cx.background_spawn(async move { drop(previous) }).detach();
        true
    }

    fn scan_ended(
        &mut self,
        answers: Vec<oneshot::Sender<()>>,
        applied: bool,
        cx: &mut Context<Self>,
    ) {
        if applied {
            for answer in answers {
                let _ = answer.send(());
            }
        }
        let waiting = self
            .scans
            .running
            .take()
            .map(|running| running.waiting)
            .unwrap_or_default();
        if !waiting.is_empty() {
            self.start_scan(waiting, cx);
        }
    }
}
