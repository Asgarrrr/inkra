//! A note's chain: every filesystem operation on it, in issue order, and its pending save.

use std::time::Duration;

use futures::channel::oneshot;
use gpui::{AsyncApp, Context, Task, WeakEntity};
use inkra_vault::EntryId;

use super::PendingSave;
use crate::vault::NoteStore;

/// Chains instead of ordering by `Writer`'s sequence: a pending write
/// decided before a rename would still carry a higher sequence and
/// recreate the note under its old name.
#[derive(Default)]
pub(in crate::vault) struct Queue {
    /// Held, never dropped: a dropped `Task` never runs, so a queued
    /// operation would vanish. `None` once the last link finished.
    chain: Option<Task<()>>,
    /// Numbers the links, so a finished one knows whether another waits behind it.
    links: u64,
    scheduled: bool,
    /// Without this, `⌘S` during a pending window is absorbed by
    /// `scheduled` and writes nothing.
    cancel: Option<oneshot::Sender<()>>,
}

impl Queue {
    /// The receiver cancels the save; `None` when a delayed save already waits. A zero delay
    /// preempts: it cancels the waiting save and takes its place.
    pub(super) fn arm(&mut self, delay: Duration) -> Option<oneshot::Receiver<()>> {
        if delay.is_zero() {
            self.cancel_pending();
        } else if self.scheduled {
            return None;
        }
        let (cancel, cancelled) = oneshot::channel();
        self.cancel = Some(cancel);
        self.scheduled = !delay.is_zero();
        Some(cancelled)
    }

    /// Any save link past its timer clears it, even with a newer delayed save behind it.
    pub(super) fn timer_passed(&mut self) {
        self.scheduled = false;
    }

    pub(super) fn is_idle(&self) -> bool {
        self.chain.is_none()
    }

    /// Detached, not dropped: the finished link is the task running this.
    pub(super) fn finish(&mut self, link: u64) {
        if link == self.links
            && let Some(last) = self.chain.take()
        {
            last.detach();
        }
    }

    /// `spawn` gets the link before it and the new link's number, so `finish` knows the last one.
    fn push(&mut self, spawn: impl FnOnce(Option<Task<()>>, u64) -> Task<()>) {
        // Taken, not dropped: replacing the slot outright would cancel an
        // operation already queued behind it.
        let previous = self.chain.take();
        self.links += 1;
        self.chain = Some(spawn(previous, self.links));
    }

    #[cfg(test)]
    pub(super) fn is_scheduled(&self) -> bool {
        self.scheduled
    }

    /// Drops the write waiting on the chain, whatever its delay; one already writing still lands.
    fn cancel_pending(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        self.scheduled = false;
    }
}

impl NoteStore {
    /// Runs `op` after every operation already queued on the note's chain, `pending` applied first,
    /// and resolves to its result.
    pub(in crate::vault) fn queue<Op>(
        &mut self,
        id: EntryId,
        pending: PendingSave,
        cx: &mut Context<Self>,
        op: Op,
    ) -> Task<bool>
    where
        Op: AsyncFnOnce(WeakEntity<Self>, &mut AsyncApp) -> bool + 'static,
    {
        match pending {
            // Preempted, not cancelled: cancelling would trash or move the note with
            // the previous save's text. A no-op for a closed note.
            PendingSave::Write => self.schedule_save(id, Duration::ZERO, cx),
            PendingSave::Drop => self.sessions.queue_mut(id).cancel_pending(),
            PendingSave::Keep => {}
        }
        let (done, wait) = oneshot::channel();
        self.link(id, cx, async move |previous, this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let _ = done.send(op(this, cx).await);
        });
        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
    }

    /// Appends `body` to the note's chain; `body` awaits the link before it, handed in.
    pub(in crate::vault) fn link<Body>(&mut self, id: EntryId, cx: &mut Context<Self>, body: Body)
    where
        Body: AsyncFnOnce(Option<Task<()>>, WeakEntity<Self>, &mut AsyncApp) + 'static,
    {
        self.sessions.queue_mut(id).push(|previous, link| {
            cx.spawn(async move |this, cx| {
                body(previous, this.clone(), cx).await;
                let _ = this.update(cx, |this, _| this.sessions.settle(id, link));
            })
        });
    }
}
