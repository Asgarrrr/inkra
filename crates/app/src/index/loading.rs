use std::future::poll_fn;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{AsyncApp, Context, Priority, WeakEntity};
use inkra_index::NoteContent;
use inkra_index::live::{Load, Ticket};

use super::IndexSync;

/// Loads in flight per core; set by the startup figure (search spec, Performance).
pub(super) const LOADS_PER_CORE: usize = 1;
/// How long one foreground turn applies landed loads before it yields: half the 8 ms step budget.
const APPLY_BUDGET: Duration = Duration::from_millis(4);

pub(super) struct Landed {
    pub(super) ticket: Ticket,
    pub(super) loaded: inkra_vault::Result<NoteContent>,
}

impl IndexSync {
    pub(super) fn load_more(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        let slots = cx.background_executor().num_cpus() * LOADS_PER_CORE;
        while let Some(Load { ticket, path }) = self.live.next_load(vault.read(cx), slots) {
            let fs = Arc::clone(&self.fs);
            let landed = self.landed.clone();
            // Low, as Zed's scanner: a query's scan goes first.
            cx.background_executor()
                .spawn_with_priority(Priority::Low, async move {
                    let loaded = fs
                        .load(&path)
                        .await
                        .map(|loaded| NoteContent::parse(loaded.text.into(), loaded.stamp.mtime));
                    // Closed only once this entity is gone, and then nobody waits for it.
                    let _ = landed.unbounded_send(Landed { ticket, loaded });
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
        while let Some(Landed { ticket, loaded }) = next {
            self.land(ticket, loaded, cx);
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
