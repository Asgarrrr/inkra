//! The one read task of the sources with no slot.

use std::collections::HashMap;
use std::pin::pin;

use futures::StreamExt as _;
use gpui::{Context, Task};
use inkra_index::{Backlink, Failures, Read, Slot};
use inkra_vault::EntryId;

use super::{BacklinksPanel, occurrences};
use crate::index::READERS;

/// Sources read per note; the rows past them show their name only.
pub(super) const READ_SOURCES: usize = 1_000;

pub(super) struct Job {
    /// In row order.
    wanted: Vec<Backlink>,
    /// `None` until the panel is drawn; dropping it stops the reads left. A task that stopped
    /// on its failures stays, so a draw does not start it again.
    task: Option<Task<()>>,
}

impl Job {
    pub(super) fn new(wanted: Vec<Backlink>) -> Self {
        Self { wanted, task: None }
    }

    /// Whether this job reads every backlink of `pending` as it is now.
    pub(super) fn covers(&self, pending: &[Backlink]) -> bool {
        let wanted: HashMap<EntryId, &Backlink> = self
            .wanted
            .iter()
            .map(|backlink| (backlink.source(), backlink))
            .collect();
        pending.iter().all(|backlink| {
            wanted
                .get(&backlink.source())
                .is_some_and(|wanted| wanted.reads_as(backlink))
        })
    }
}

impl BacklinksPanel {
    /// Starts the job's reads once; a hidden panel is never drawn, so it reads nothing.
    pub(super) fn start_reads(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.job.as_mut().filter(|job| job.task.is_none()) else {
            return;
        };
        let items = job
            .wanted
            .iter()
            .map(|backlink| (backlink.candidate().clone(), backlink.clone()))
            .collect();
        let reader = self.links.read(cx).reader(cx);
        let reads = reader
            .stream_unordered_with(items, |backlink, read| (backlink, read))
            .map(|(_, item)| item)
            .ready_chunks(READERS);
        job.task = Some(cx.spawn(async move |this, cx| {
            let mut reads = pin!(reads);
            let mut failures = Failures::default();
            while let Some(batch) = reads.next().await {
                let stop = this.update(cx, |panel, cx| panel.land_batch(batch, &mut failures, cx));
                // A dropped panel stops it too.
                if stop.unwrap_or(true) {
                    return;
                }
            }
        }));
    }

    /// The reads ready now, landed as one update; an item whose row's backlink no longer
    /// reads as its own is dropped, as a rebuild replaced it. Whether the job stops.
    fn land_batch(
        &mut self,
        batch: Vec<(Backlink, Read)>,
        failures: &mut Failures,
        cx: &mut Context<Self>,
    ) -> bool {
        let (mut stale, mut forget) = (Vec::new(), Vec::new());
        let (mut changed, mut stop) = (false, false);
        for (backlink, read) in batch {
            let Some(row) = self
                .rows
                .iter_mut()
                .find(|row| row.backlink.reads_as(&backlink))
            else {
                continue;
            };
            let id = backlink.source();
            let landed = Slot::land(self.slots.get(&id), &backlink, &read);
            let occurrences = occurrences(&backlink, Some(&landed.slot));
            changed |= row.occurrences != occurrences;
            row.occurrences = occurrences;
            stop |= failures.count(&landed.slot);
            self.slots.insert(id, landed.slot);
            if landed.stale {
                stale.push(id);
            }
            if landed.forget {
                forget.push((id, backlink.candidate().stamp));
            }
        }
        self.links.update(cx, |links, cx| {
            for (id, stamp) in forget {
                links.forget(id, stamp);
            }
            if !stale.is_empty() {
                links.stale(&stale, cx);
            }
        });
        if changed {
            cx.notify();
        }
        stop
    }
}
