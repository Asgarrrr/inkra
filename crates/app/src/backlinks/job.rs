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

/// A source with no slot, and the index of its row in the panel's rows.
pub(super) struct Wanted {
    pub(super) row: usize,
    pub(super) backlink: Backlink,
}

pub(super) struct Job {
    /// Each backlink still to land; every rebuild that keeps the job sets it to its pending set,
    /// so each `row` indexes the rows as they are.
    wanted: HashMap<EntryId, Wanted>,
    /// `None` until the panel is drawn; dropping it stops the reads left.
    task: Option<Task<()>>,
}

impl Job {
    pub(super) fn new(wanted: HashMap<EntryId, Wanted>) -> Self {
        Self { wanted, task: None }
    }

    /// Whether this job will still read every backlink of `pending` as it is now.
    pub(super) fn covers(&self, pending: &HashMap<EntryId, Wanted>) -> bool {
        pending.iter().all(|(id, pending)| {
            self.wanted
                .get(id)
                .is_some_and(|wanted| wanted.backlink.reads_as(&pending.backlink))
        })
    }

    /// Keeps the job for `pending`, which it covers: the slots a rebuild filled are not read.
    pub(super) fn narrow(&mut self, pending: HashMap<EntryId, Wanted>) {
        self.wanted = pending;
    }
}

impl BacklinksPanel {
    /// Starts the job's reads once; a hidden panel is never drawn, so it reads nothing.
    pub(super) fn start_reads(&mut self, cx: &mut Context<Self>) {
        if self.failures.stopped() {
            return;
        }
        let Some(job) = self.job.as_mut().filter(|job| job.task.is_none()) else {
            return;
        };
        let mut wanted: Vec<&Wanted> = job.wanted.values().collect();
        wanted.sort_unstable_by_key(|wanted| wanted.row);
        let items = wanted
            .into_iter()
            .map(|wanted| {
                (
                    wanted.backlink.candidate().clone(),
                    wanted.backlink.source(),
                )
            })
            .collect();
        let reader = self.links.read(cx).reader(cx);
        let reads = reader
            .stream_unordered_with(items, |id, read| (id, read))
            .map(|(_, item)| item)
            .ready_chunks(READERS);
        job.task = Some(cx.spawn(async move |this, cx| {
            let mut reads = pin!(reads);
            while let Some(batch) = reads.next().await {
                let stop = this.update(cx, |panel, cx| panel.land_batch(batch, cx));
                // A dropped panel stops it too.
                if stop.unwrap_or(true) {
                    return;
                }
            }
        }));
    }

    /// The reads ready now, landed as one update; a source the job no longer wants is dropped,
    /// as a rebuild filled or replaced it. Whether the reads stop.
    fn land_batch(&mut self, batch: Vec<(EntryId, Read)>, cx: &mut Context<Self>) -> bool {
        let Some(job) = self.job.as_mut() else {
            return true;
        };
        let (mut stale, mut forget) = (Vec::new(), Vec::new());
        let mut changed = false;
        for (id, read) in batch {
            let Some(Wanted { row, backlink }) = job.wanted.remove(&id) else {
                continue;
            };
            let landed = Slot::land(self.slots.get(&id), &backlink, &read);
            let occurrences = occurrences(&backlink, Some(&landed.slot));
            let row = &mut self.rows[row];
            changed |= row.occurrences != occurrences;
            row.occurrences = occurrences;
            self.failures.count(&landed.slot);
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
        self.failures.stopped()
    }
}
