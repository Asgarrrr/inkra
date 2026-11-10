//! The one read task of the sources with no slot.

use std::collections::HashSet;
use std::pin::pin;

use futures::StreamExt as _;
use gpui::{Context, Task};
use inkra_index::{Backlink, Read, Slot};

use super::{BacklinksPanel, occurrences};

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

    /// Whether this job reads every source of `pending`.
    pub(super) fn covers(&self, pending: &[Backlink]) -> bool {
        let wanted: HashSet<_> = self.wanted.iter().map(Backlink::source).collect();
        pending
            .iter()
            .all(|backlink| wanted.contains(&backlink.source()))
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
        let reads = reader.stream_with(items, |backlink, read| (backlink, read));
        job.task = Some(cx.spawn(async move |this, cx| {
            let mut reads = pin!(reads);
            while let Some((_, (backlink, read))) = reads.next().await {
                if this
                    .update(cx, |panel, cx| panel.land(&backlink, &read, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    /// Ignored unless `backlink` is still its row's: a rebuild may have replaced it.
    fn land(&mut self, backlink: &Backlink, read: &Read, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter_mut().find(|row| row.backlink == *backlink) else {
            return;
        };
        let slot = Slot::land(backlink, read);
        row.occurrences = occurrences(backlink, Some(&slot));
        self.slots.insert(backlink.source(), slot);
        cx.notify();
    }
}
