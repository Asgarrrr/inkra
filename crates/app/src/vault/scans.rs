//! One scan worker per store: serial scans, coalesced requests, replayed re-keys (open-first spec, O2).

use std::sync::Arc;

use futures::channel::oneshot;
use gpui::{BackgroundExecutor, Context, Entity, Priority, prelude::*};
use inkra_vault::{
    EntryId, FileIdentity, Job, Jobs, Mtime, Pending, Reach, RelPath, Result, Vault, Walk,
};

use super::NoteStore;
use crate::timeline::{Step, Timeline, timed};

#[derive(Default)]
pub(super) struct Scans {
    /// Between a scan's start and its swap.
    running: Option<Running>,
}

#[derive(Default)]
struct Running {
    /// The scan after this one, which starts once this one has swapped.
    next: Next,
    /// Re-keys since the running scan took its base, replayed at its swap.
    reidentified: Vec<Rekey>,
}

/// What a scan reads and who waits for it: only a request waits, and a request reads everything.
pub(super) enum Next {
    /// A watch's batches, merged (watcher spec, F4).
    Batch(Pending),
    /// `rescan()` requests, answered once the scan swaps in.
    Rescan(Vec<oneshot::Sender<()>>),
}

impl Default for Next {
    fn default() -> Self {
        Self::Batch(Pending::default())
    }
}

/// A save's re-key, queued during a scan.
struct Rekey {
    id: EntryId,
    identity: FileIdentity,
    mtime: Mtime,
    /// The live vault already mapped `identity` to `id`: the save kept the inode (watcher spec, F5).
    in_place: bool,
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
                Some(Running {
                    next: Next::Rescan(waiting),
                    ..
                }) => waiting.push(applied),
                Some(running) => running.next = Next::Rescan(vec![applied]),
                None => self.start_scan(Next::Rescan(vec![applied]), cx),
            }
        }
        wait
    }

    /// A watch's batch: read now, or merged into what the scan after the running one reads.
    pub(super) fn rescan_changed(&mut self, batch: Pending, cx: &mut Context<Self>) {
        if batch.is_empty() {
            return;
        }
        match &mut self.scans.running {
            Some(Running {
                next: Next::Batch(pending),
                ..
            }) => pending.merge(batch),
            // A request reads everything already.
            Some(_) => {}
            None => self.start_scan(Next::Batch(batch), cx),
        }
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
            let in_place = self
                .vault
                .as_ref()
                .is_some_and(|vault| vault.read(cx).id_for(identity) == Some(id));
            running.reidentified.push(Rekey {
                id,
                identity,
                mtime,
                in_place,
            });
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

    /// A vault no walk has read whole is read whole, whatever `next` reads (watcher spec, F4).
    pub(super) fn start_scan(&mut self, next: Next, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let (answers, read) = match next {
            Next::Batch(read) => (Vec::new(), read),
            Next::Rescan(waiting) => (waiting, Pending::Everything),
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
                            ..Walk::default()
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
                            this.swap(&vault, listed, true, Pass::Root, cx),
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
                    let walk = match (seed, read) {
                        (Some(seed), _) => Vault::walk_seeded(&*fs, &root, seed, &jobs).await,
                        (None, Pending::Folders(folders)) if base.has_read_whole() => {
                            base.walk_changed(&*fs, &folders, &jobs).await
                        }
                        (None, _) => Vault::walk(&*fs, &root, &everything, &jobs).await?,
                    };
                    let incomplete = walk.reach == Reach::Incomplete;
                    let mut next = base.applied(walk);
                    next.mark_scanned();
                    let changed = next != base;
                    Ok((next, changed, incomplete))
                })
                .await;
            let (scanned, changed, incomplete) = match scanned {
                Ok((scanned, changed, incomplete)) => (Ok(scanned), changed, incomplete),
                Err(error) => (Err(error), false, false),
            };
            let _ = timed(cx, |cx| {
                this.update(cx, |this, cx| {
                    let applied = this.swap(&vault, scanned, changed, Pass::Full, cx);
                    this.scan_ended(answers, applied, incomplete, cx);
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
    /// `changed`: the result differs from the scan's base.
    fn swap(
        &mut self,
        vault: &Entity<Vault>,
        scanned: Result<Vault>,
        changed: bool,
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
                    for rekey in reidentified {
                        moved |= vault.reidentify(rekey.id, rekey.identity, rekey.mtime);
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
        // First, so the replayed re-key of a save to a gone note finds its id (watcher spec, F9).
        let mut changed = self.reclaim_gone(&mut next, vault.read(cx)) || changed;
        for rekey in reidentified {
            changed |=
                next.replay_reidentify(rekey.id, rekey.identity, rekey.mtime, rekey.in_place);
        }
        let previous = vault.update(cx, |vault, cx| {
            let previous = std::mem::replace(vault, next);
            // Our own save's echo changes nothing: the sidebar and the index skip their O(n) pass.
            if changed {
                cx.notify();
            }
            previous
        });
        self.check_open_notes(vault, &previous, cx);
        // The replaced state may be the last owner of 50,000 entries: freeing them is foreground time.
        cx.background_spawn(async move { drop(previous) }).detach();
        true
    }

    /// `incomplete`: a scoped read failed, so the next scan reads everything; a full scan never sets it.
    fn scan_ended(
        &mut self,
        answers: Vec<oneshot::Sender<()>>,
        applied: bool,
        incomplete: bool,
        cx: &mut Context<Self>,
    ) {
        if applied {
            for answer in answers {
                let _ = answer.send(());
            }
        }
        let Some(running) = self.scans.running.take() else {
            return;
        };
        let next = match running.next {
            Next::Batch(_) if incomplete => Next::Batch(Pending::Everything),
            Next::Batch(pending) if pending.is_empty() => return,
            next => next,
        };
        self.start_scan(next, cx);
    }
}
