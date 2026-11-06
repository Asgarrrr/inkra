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
    pub(super) async fn drain(
        this: WeakEntity<Self>,
        mut arrivals: UnboundedReceiver<Landed>,
        cx: &mut AsyncApp,
    ) {
        while let Some(first) = arrivals.next().await {
            let alive = this.update(cx, |this, cx| this.apply_landed(first, &mut arrivals, cx));
            if alive.is_err() {
                return;
            }
            // Loads that landed meanwhile would otherwise keep this turn going past its budget.
            yield_now().await;
        }
    }

    pub(super) fn spawn_load(&self, Load { ticket, path }: Load, cx: &mut Context<Self>) {
        let fs = Arc::clone(&self.fs);
        let landed = self.landed.clone();
        // Low, as Zed's scanner: a query's scan goes first.
        cx.background_executor()
            .spawn_with_priority(Priority::Low, async move {
                let loaded = fs
                    .load(&path)
                    .await
                    .map(|loaded| NoteContent::parse(&loaded.text, loaded.stamp));
                // Closed only once this entity is gone, and then nobody waits for it.
                let _ = landed.unbounded_send(Landed { ticket, loaded });
            })
            .detach();
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
        #[cfg(test)]
        {
            self.turns += 1;
        }
        self.turn(cx, |turn| {
            let started = Instant::now();
            let mut next = Some(first);
            while let Some(Landed { ticket, loaded }) = next {
                turn.landed(ticket, loaded);
                next = if started.elapsed() < budget {
                    arrivals.try_recv().ok()
                } else {
                    None
                };
            }
        });
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
