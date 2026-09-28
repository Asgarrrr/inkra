use std::future::poll_fn;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{AsyncApp, Context, Priority, WeakEntity};
use inkra_index::NoteContent;
use inkra_vault::EntryId;

use super::IndexSync;

/// Loads in flight per core; set by the startup figure (search spec, Performance).
pub(super) const LOADS_PER_CORE: usize = 1;
/// How long one foreground turn applies landed loads before it yields: half the 8 ms step budget.
const APPLY_BUDGET: Duration = Duration::from_millis(4);

/// A load started and not applied yet.
pub(super) struct Loading {
    pub(super) id: EntryId,
    seq: u64,
    /// False once its note left the vault: not pending, though it keeps its slot until it lands.
    pub(super) live: bool,
}

pub(super) struct Landed {
    pub(super) id: EntryId,
    pub(super) seq: u64,
    /// `note_sets` when the load started.
    pub(super) note_sets: u64,
    pub(super) loaded: inkra_vault::Result<NoteContent>,
}

impl IndexSync {
    pub(super) fn load_more(&mut self, cx: &mut Context<Self>) {
        let slots = cx.background_executor().num_cpus() * LOADS_PER_CORE;
        while self.loading.len() < slots {
            let Some((id, seq, path)) = self.next_load(cx) else {
                return;
            };
            self.loading.push(Loading {
                id,
                seq,
                live: true,
            });
            let fs = Arc::clone(&self.fs);
            let landed = self.landed.clone();
            let note_sets = self.note_sets;
            // Low, as Zed's scanner: a query's scan goes first.
            cx.background_executor()
                .spawn_with_priority(Priority::Low, async move {
                    let loaded = fs
                        .load(&path)
                        .await
                        .map(|loaded| NoteContent::parse(loaded.text.into(), loaded.stamp.mtime));
                    // Closed only once this entity is gone, and then nobody waits for it.
                    let _ = landed.unbounded_send(Landed {
                        id,
                        seq,
                        note_sets,
                        loaded,
                    });
                })
                .detach();
        }
    }

    pub(super) async fn drain(
        this: WeakEntity<Self>,
        mut arrivals: UnboundedReceiver<Landed>,
        cx: &mut AsyncApp,
    ) {
        while let Some(first) = arrivals.next().await {
            let alive = Self::timed_turn(&this, cx, |this, cx| {
                this.apply_landed(first, &mut arrivals, cx);
            });
            if alive.is_none() {
                return;
            }
            // Loads that landed meanwhile would otherwise keep this turn going past its budget.
            yield_now().await;
        }
    }

    /// One note's apply is never split, so the heaviest note bounds a turn; `longest_turn` shows it.
    fn apply_landed(
        &mut self,
        first: Landed,
        arrivals: &mut UnboundedReceiver<Landed>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(not(test))]
        let budget = APPLY_BUDGET;
        #[cfg(test)]
        let budget = self.apply_budget.unwrap_or(APPLY_BUDGET);
        let started = Instant::now();
        let mut next = Some(first);
        while let Some(Landed {
            id,
            seq,
            note_sets,
            loaded,
        }) = next
        {
            if let Some(at) = self
                .loading
                .iter()
                .position(|load| load.id == id && load.seq == seq)
            {
                self.loading.swap_remove(at);
            }
            match loaded {
                Ok(content) => self.apply(id, seq, content, cx),
                Err(_) => self.failed(id, seq, note_sets, cx),
            }
            next = if started.elapsed() < budget {
                arrivals.try_recv().ok()
            } else {
                None
            };
        }
        #[cfg(test)]
        {
            self.turns += 1;
        }
        self.load_more(cx);
        cx.notify();
    }

    /// A build since the load started may have moved the note; a rename keeps its mtime, so no rescan reloads it.
    fn failed(&mut self, id: EntryId, seq: u64, note_sets: u64, cx: &mut Context<Self>) {
        let present = self
            .vault
            .as_ref()
            .is_some_and(|vault| vault.read(cx).entry(id).is_some());
        if note_sets != self.note_sets && present && self.is_latest(id, seq) {
            self.queue_load(id);
        } else {
            // `forget` records the note in `forgotten`, so the next change of either kind reloads it.
            self.forget(id, seq, cx);
        }
    }

    fn next_load(&mut self, cx: &Context<Self>) -> Option<(EntryId, u64, PathBuf)> {
        let vault = self.vault.as_ref()?.read(cx);
        let mut busy = Vec::new();
        let next = loop {
            let Some((id, seq)) = self.queue.pop_front() else {
                break None;
            };
            if self.queued.get(&id) != Some(&seq) {
                continue;
            }
            // One read per note at a time, so a note rewritten in a loop cannot take every slot.
            if self.loading.iter().any(|load| load.id == id) {
                busy.push((id, seq));
                continue;
            }
            self.queued.remove(&id);
            if let Some(path) = vault.absolute(id) {
                break Some((id, seq, path));
            }
        };
        for entry in busy.into_iter().rev() {
            self.queue.push_front(entry);
        }
        next
    }
}

/// Pending once, so the task goes to the back of the foreground queue.
async fn yield_now() {
    let mut yielded = false;
    poll_fn(|task| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        task.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}
