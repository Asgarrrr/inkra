//! A picker's runs over the index: which one is current, when a refresh starts, and what lands.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use gpui::{Context, Entity, Priority, Subscription};
use inkra_index::live::Status;
use inkra_index::{IndexSnapshot, NoteQuery};
use inkra_vault::Jobs;

use super::{Arrival, Picker, PickerDelegate, Update};
use crate::background::Background;
use crate::index::{IndexSync, IndexWatch};

mod ledger;

use ledger::{Ledger, Started};
#[cfg(test)]
pub(crate) use ledger::{NOTES_CHANGED_REFRESH, SEARCH_DEBOUNCE};
pub(crate) use ledger::{Run, RunId};

/// A picker delegate whose rows answer a query over the index; `Runs` drives its runs.
pub(crate) trait IndexDelegate: PickerDelegate {
    /// What one run found, applied on the foreground.
    type Found: Send + 'static;
    /// A row's identity across refreshes: what a refresh keeps selected.
    type Key: PartialEq;

    fn runs(&mut self) -> &mut Runs;
    fn sync(&self) -> &Entity<IndexSync>;
    /// The query `text` runs; `None` once the delegate set rows that need no scan.
    fn query(&mut self, text: &str, run: Run) -> Option<NoteQuery>;
    /// Off the foreground: `find_notes` or `search`, with the delegate's limit.
    fn search(
        snapshot: &IndexSnapshot,
        query: &NoteQuery,
        jobs: &dyn Jobs,
        cancel: &AtomicBool,
    ) -> impl Future<Output = Option<Self::Found>> + Send;
    /// Replaces the rows with a run's answer, before the picker lands it.
    fn show(&mut self, landed: Landed<Self::Found>, cx: &mut Context<Picker<Self>>);
    fn key(&self, ix: usize) -> Option<Self::Key>;
}

/// A run's answer: `Settled` when the delegate already set rows that need no scan.
pub(crate) enum Ran<F> {
    Settled,
    Found(Landed<F>),
}

/// A run's result, and what it answered.
pub(crate) struct Landed<F> {
    pub(crate) id: RunId,
    pub(crate) run: Run,
    pub(crate) begun: Instant,
    pub(crate) snapshot: IndexSnapshot,
    pub(crate) query: Arc<NoteQuery>,
    pub(crate) found: F,
}

#[derive(Default)]
pub(crate) struct Runs {
    ledger: Ledger,
    /// Loads add text, so rows; a notify refreshes at most every `BUILDING_REFRESH` while they land.
    watch: IndexWatch,
    /// Set by a run before the first build; cleared when a run lands or the delegate needs no scan.
    scanning: bool,
    /// The cancel flag each scan received, in order, so a test reads the flag the scan holds.
    #[cfg(test)]
    begun: Vec<Arc<AtomicBool>>,
    /// Each scan waits for it before it starts, so a test can hold one running.
    #[cfg(test)]
    hold: Option<Hold>,
}

#[cfg(test)]
pub(crate) type Hold = futures::future::Shared<futures::channel::oneshot::Receiver<()>>;

impl Runs {
    fn start(&mut self, sync: &IndexSync) -> Started {
        let started = self.ledger.start();
        self.watch.seen(sync);
        started
    }

    /// Refreshes wait until the started run applies; its scan awaits the returned future (tests hold it).
    fn begin_scan(&mut self, started: &Started) -> impl Future<Output = ()> + use<> {
        self.ledger.begin_scan(started);
        #[cfg(test)]
        let hold = {
            self.begun.push(Arc::clone(&started.cancel));
            self.hold.clone()
        };
        async move {
            #[cfg(test)]
            if let Some(hold) = hold {
                let _ = hold.await;
            }
        }
    }

    pub(crate) fn scanning(&self) -> bool {
        self.scanning
    }

    /// The whole of an `IndexDelegate`'s `update_matches`.
    pub(crate) fn update_matches<D: IndexDelegate>(
        delegate: &mut D,
        text: &str,
        cx: &mut Context<Picker<D>>,
    ) -> Update<Ran<D::Found>> {
        let sync = delegate.sync().clone();
        let index_sync = sync.read(cx);
        let started = delegate.runs().start(index_sync);
        if index_sync.status() == Status::Scanning {
            // Nothing to list yet; the first build's notify runs the query again.
            delegate.runs().scanning = true;
            return Update::Ready(Ran::Settled);
        }
        let Some(query) = delegate.query(text, started.run) else {
            delegate.runs().scanning = false;
            return Update::Ready(Ran::Settled);
        };
        let released = delegate.runs().begin_scan(&started);
        let Started {
            id,
            run,
            wait,
            cancel,
        } = started;
        let (snapshot, query) = (sync.read(cx).index().snapshot(), Arc::new(query));
        let jobs = Background {
            executor: cx.background_executor().clone(),
            priority: Priority::default(),
        };
        let begun = Instant::now();
        // Off the foreground: dropping it drops the scans' scope on a background thread.
        Update::Pending(cx.background_executor().spawn(async move {
            if let Some(wait) = wait {
                jobs.executor.timer(wait).await;
            }
            released.await;
            let found = D::search(&snapshot, &query, &jobs, &cancel).await?;
            Some(Ran::Found(Landed {
                id,
                run,
                begun,
                snapshot,
                query,
                found,
            }))
        }))
    }

    /// Shows what a run found, keeping the selection on a refresh.
    pub(crate) fn apply_matches<D: IndexDelegate>(
        delegate: &mut D,
        ran: Ran<D::Found>,
        selected: Option<usize>,
        cx: &mut Context<Picker<D>>,
    ) -> Arrival {
        let Ran::Found(landed) = ran else {
            return Arrival::first();
        };
        // Read at landing, not when the refresh started: the user may have moved meanwhile.
        let kept = selected
            .filter(|_| !landed.run.is_user())
            .and_then(|ix| delegate.key(ix));
        delegate.runs().scanning = false;
        delegate.show(landed, cx);
        let row = kept.and_then(|kept| {
            (0..delegate.match_count()).find(|&ix| delegate.key(ix).as_ref() == Some(&kept))
        });
        let ledger = &mut delegate.runs().ledger;
        let rerun = ledger
            .landed()
            .and_then(|run| ledger.refresh(run))
            .is_some();
        match row {
            Some(row) => Arrival {
                row,
                keep_scroll: true,
                rerun,
            },
            // A new query, or the kept row is gone: the first row, scrolled to.
            None => Arrival {
                rerun,
                ..Arrival::first()
            },
        }
    }

    /// Search spec D7: a refresh never cancels a running scan, whose query the user still waits on.
    pub(crate) fn refresh_or_defer<D: IndexDelegate>(
        picker: &mut Picker<D>,
        run: Run,
        cx: &mut Context<Picker<D>>,
    ) {
        let Some(run) = picker.delegate.runs().ledger.refresh(run) else {
            return;
        };
        if run.is_user() {
            picker.refresh(cx);
        } else {
            picker.refresh_in_background(cx);
        }
    }

    /// Refreshes on `sync`'s notifies: at once when built, throttled while loads land.
    pub(crate) fn observe<D: IndexDelegate>(
        sync: &Entity<IndexSync>,
        cx: &mut Context<Picker<D>>,
    ) -> Subscription {
        IndexWatch::observe(
            sync,
            cx,
            |picker| &mut picker.delegate.runs().watch,
            |picker, cx| Self::refresh_or_defer(picker, Run::Notify, cx),
        )
    }

    /// `None` releases the scans started from now on.
    #[cfg(test)]
    pub(crate) fn hold(&mut self, hold: Option<Hold>) {
        self.hold = hold;
    }

    #[cfg(test)]
    pub(crate) fn begun(&self) -> &[Arc<AtomicBool>] {
        &self.begun
    }
}
