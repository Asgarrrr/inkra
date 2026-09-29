//! A picker's runs over the index: which one is current, when a refresh starts, and what lands.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{Context, Entity, Priority, Subscription, Task};
use inkra_index::{IndexSnapshot, NoteQuery};
use inkra_vault::Jobs;

use super::{Picker, PickerDelegate};
use crate::background::Background;
use crate::index::{IndexSync, IndexWatch};

/// Waited only behind a user search, so an idle or refreshing picker answers at once.
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);
/// After the names change, as Zed's file finder waits after a worktree update.
pub(crate) const NOTES_CHANGED_REFRESH: Duration = Duration::from_millis(200);

/// A picker delegate whose rows answer a query over the index; `Runs` drives its runs.
pub(crate) trait IndexDelegate: PickerDelegate {
    /// What one run found, applied on the foreground.
    type Found: Send + 'static;
    /// A row's identity across refreshes: what a refresh keeps selected.
    type Key: PartialEq;

    fn runs(&mut self) -> &mut Runs;
    fn sync(&self) -> &Entity<IndexSync>;
    /// Before the first build: empty rows that say so.
    fn scanning(&mut self);
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

/// A scan in flight. Search spec D7: a refresh never cancels it; it marks it for a re-run instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scan {
    /// Only behind a user run does a keystroke wait.
    pub(crate) user: bool,
    /// A refresh arrived during the scan: one re-run follows once it applies.
    pub(crate) rerun: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Run {
    /// A keystroke, or the first results after the scan: the user waits on it.
    #[default]
    Search,
    /// After the notes changed: waits `NOTES_CHANGED_REFRESH`, keeps the selected row if still listed.
    Refresh,
    /// After a notify or a deferred refresh: keeps the selection and waits nothing.
    Notify,
}

impl Run {
    pub(crate) fn is_user(self) -> bool {
        self == Self::Search
    }

    /// How long the run waits before its scan.
    fn wait(self, behind_user: bool) -> Option<Duration> {
        match self {
            Self::Search => behind_user.then_some(SEARCH_DEBOUNCE),
            Self::Refresh => Some(NOTES_CHANGED_REFRESH),
            Self::Notify => None,
        }
    }
}

/// Which run a result answers: only the current one applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RunId(u64);

impl RunId {
    #[cfg(test)]
    pub(crate) fn previous(self) -> Self {
        Self(self.0 - 1)
    }
}

#[derive(Default)]
pub(crate) struct Runs {
    /// Set by the next run and on drop: a scan already running on another thread ignores a dropped task.
    cancel: Arc<AtomicBool>,
    id: RunId,
    scan: Option<Scan>,
    /// Set by a refresh for the next run only.
    next: Run,
    /// Loads add text, so rows; a notify refreshes at most every `BUILDING_REFRESH` while they land.
    watch: IndexWatch,
    /// Scans started, the scanning state's empty runs aside.
    #[cfg(test)]
    started: usize,
    /// Each scan waits for it before it starts, so a test can hold one running.
    #[cfg(test)]
    hold: Option<Hold>,
}

#[cfg(test)]
pub(crate) type Hold = futures::future::Shared<futures::channel::oneshot::Receiver<()>>;

/// The run `Runs::start` begins.
#[must_use]
struct Started {
    id: RunId,
    cancel: Arc<AtomicBool>,
    run: Run,
    /// A keystroke supersedes a background run at once; it waits only behind a user search.
    behind_user: bool,
}

impl Drop for Runs {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Runs {
    /// Supersedes the run before: cancels its scan and takes the run a refresh set.
    fn start(&mut self, sync: &IndexSync) -> Started {
        std::mem::take(&mut self.cancel).store(true, Ordering::Relaxed);
        self.id = RunId(self.id.0 + 1);
        let run = std::mem::take(&mut self.next);
        // This run reads the newest index, whatever made the scan before it stale.
        let behind_user = std::mem::take(&mut self.scan).is_some_and(|scan| scan.user);
        self.watch.seen(sync);
        Started {
            id: self.id,
            cancel: Arc::clone(&self.cancel),
            run,
            behind_user,
        }
    }

    /// Refreshes wait until the started run applies; its scan awaits the returned future (tests hold it).
    fn begin_scan(&mut self, started: &Started) -> impl Future<Output = ()> + use<> {
        self.scan = Some(Scan {
            user: started.run.is_user(),
            rerun: false,
        });
        #[cfg(test)]
        let hold = {
            self.started += 1;
            self.hold.clone()
        };
        async move {
            #[cfg(test)]
            if let Some(hold) = hold {
                let _ = hold.await;
            }
        }
    }

    fn is_current(&self, id: RunId) -> bool {
        id == self.id
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
        if index_sync.awaits_first_build() {
            // Nothing to list yet; the first build's notify runs the query again.
            delegate.scanning();
            return Self::settle(started.id, cx);
        }
        let Some(query) = delegate.query(text, started.run) else {
            return Self::settle(started.id, cx);
        };
        let wait = started.run.wait(started.behind_user);
        let released = delegate.runs().begin_scan(&started);
        let Started {
            id, cancel, run, ..
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
        if !picker.delegate.runs().is_current(landed.id) {
            return;
        }
        // Read now, not when the refresh started: the user may have moved meanwhile.
        let kept = picker
            .selected()
            .filter(|_| !landed.run.is_user())
            .and_then(|ix| picker.delegate.key(ix));
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
                if picker.delegate.runs().is_current(id) {
                    picker.matches_updated(0, cx);
                }
            });
        })
    }

    /// The current run's rows landed; a refresh deferred behind it starts now, unless the picker confirmed.
    fn landed<D: IndexDelegate>(picker: &mut Picker<D>, cx: &mut Context<Picker<D>>) {
        let scan = picker.delegate.runs().scan.take();
        if scan.is_some_and(|scan| scan.rerun) && !picker.confirmed {
            Self::refresh_or_defer(picker, Run::Notify, cx);
        }
    }

    /// Search spec D7: a refresh never cancels a running scan, whose query the user still waits on.
    pub(crate) fn refresh_or_defer<D: IndexDelegate>(
        picker: &mut Picker<D>,
        run: Run,
        cx: &mut Context<Picker<D>>,
    ) {
        let runs = picker.delegate.runs();
        if let Some(scan) = &mut runs.scan {
            scan.rerun = true;
        } else {
            runs.next = run;
            if run.is_user() {
                picker.refresh(cx);
            } else {
                picker.refresh_in_background(cx);
            }
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
    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    #[cfg(test)]
    pub(crate) fn id(&self) -> RunId {
        self.id
    }

    #[cfg(test)]
    pub(crate) fn scan(&self) -> Option<Scan> {
        self.scan
    }

    #[cfg(test)]
    pub(crate) fn started(&self) -> usize {
        self.started
    }
}
