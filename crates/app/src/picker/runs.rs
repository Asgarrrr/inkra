//! A picker's runs over the index: which one is current, when a refresh starts, and what lands.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use gpui::{Context, Entity, Priority, Subscription, Task};
use inkra_index::live::Status;
use inkra_index::{IndexSnapshot, NoteQuery};
use inkra_vault::Jobs;

use super::{Picker, PickerDelegate};
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
    /// Replaces the rows with what `landed` found.
    fn show(&mut self, landed: Landed<Self::Found>, cx: &mut Context<Picker<Self>>);
    fn key(&self, ix: usize) -> Option<Self::Key>;
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
    ) -> Task<()> {
        let sync = delegate.sync().clone();
        let index_sync = sync.read(cx);
        let started = delegate.runs().start(index_sync);
        if index_sync.status() == Status::Scanning {
            // Nothing to list yet; the first build's notify runs the query again.
            delegate.runs().scanning = true;
            return Self::settle(started.id, cx);
        }
        let Some(query) = delegate.query(text, started.run) else {
            delegate.runs().scanning = false;
            return Self::settle(started.id, cx);
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
        cx.spawn(async move |picker, cx| {
            if let Some(wait) = wait {
                cx.background_executor().timer(wait).await;
            }
            released.await;
            let (scanned, asked) = (snapshot.clone(), Arc::clone(&query));
            // Its own task: dropping this one drops the scans' scope on a background thread.
            let found = cx
                .background_executor()
                .spawn(async move { D::search(&scanned, &asked, &jobs, &cancel).await });
            let Some(found) = found.await else {
                return;
            };
            let landed = Landed {
                id,
                run,
                begun,
                snapshot,
                query,
                found,
            };
            let _ = picker.update(cx, |picker, cx| Self::apply(picker, landed, cx));
        })
    }

    /// Shows the current run's result, keeping the selection on a refresh; a stale one is dropped.
    pub(crate) fn apply<D: IndexDelegate>(
        picker: &mut Picker<D>,
        landed: Landed<D::Found>,
        cx: &mut Context<Picker<D>>,
    ) {
        if !picker.delegate.runs().ledger.is_current(landed.id) {
            return;
        }
        // Read now, not when the refresh started: the user may have moved meanwhile.
        let kept = picker
            .selected()
            .filter(|_| !landed.run.is_user())
            .and_then(|ix| picker.delegate.key(ix));
        picker.delegate.runs().scanning = false;
        picker.delegate.show(landed, cx);
        let delegate = &picker.delegate;
        let row = kept.and_then(|kept| {
            (0..delegate.match_count()).find(|&ix| delegate.key(ix).as_ref() == Some(&kept))
        });
        match row {
            Some(ix) => picker.matches_refreshed(ix, cx),
            // A new query, or the kept row is gone: the first row, scrolled to.
            None => picker.matches_updated(0, cx),
        }
        // After the rows land: an `enter` waiting on them confirms before a re-run starts.
        Self::landed(picker, cx);
    }

    /// Lands a run that scans nothing: the delegate already set its rows.
    fn settle<D: IndexDelegate>(id: RunId, cx: &mut Context<Picker<D>>) -> Task<()> {
        cx.spawn(async move |picker, cx| {
            let _ = picker.update(cx, |picker, cx| {
                if picker.delegate.runs().ledger.is_current(id) {
                    picker.matches_updated(0, cx);
                }
            });
        })
    }

    /// The current run's rows landed; a refresh deferred behind it starts now, unless the picker confirmed.
    fn landed<D: IndexDelegate>(picker: &mut Picker<D>, cx: &mut Context<Picker<D>>) {
        if let Some(run) = picker.delegate.runs().ledger.landed()
            && !picker.confirmed
        {
            Self::refresh_or_defer(picker, run, cx);
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
