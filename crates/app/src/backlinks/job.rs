//! The one read task of the sources with no slot.

use std::collections::HashMap;
use std::pin::pin;

use futures::StreamExt as _;
use gpui::{Context, Task};
use inkra_index::{Backlink, Read, Slot};
use inkra_vault::EntryId;

use super::{BacklinksPanel, occurrences};
use crate::index::READERS;

/// Sources read per note; the rows past them show their name only.
pub(super) const READ_SOURCES: usize = 1_000;

pub(super) struct Job {
    /// In row order.
    wanted: Vec<Backlink>,
    /// `None` until the panel is drawn; dropping it stops the reads left.
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
            while let Some(batch) = reads.next().await {
                if this
                    .update(cx, |panel, cx| panel.land_batch(batch, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    /// The reads ready now, landed as one update; an item whose row's backlink no longer
    /// reads as its own is dropped, as a rebuild replaced it.
    fn land_batch(&mut self, batch: Vec<(Backlink, Read)>, cx: &mut Context<Self>) {
        let mut stale = Vec::new();
        let mut changed = false;
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
            self.slots.insert(id, landed.slot);
            if landed.stale {
                stale.push(id);
            }
        }
        if !stale.is_empty() {
            self.links.update(cx, |links, cx| links.stale(&stale, cx));
        }
        if changed {
            cx.notify();
        }
    }
}
