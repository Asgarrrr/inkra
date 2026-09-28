//! A picker delegate's background runs: which one is current, and when a refresh starts.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Context, Entity, Subscription, Task};

use super::{Picker, PickerDelegate};
use crate::index::{BUILDING_REFRESH, IndexSync, IndexWatch, Refresh};

/// Waited only behind a user search, so an idle or refreshing picker answers at once.
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);

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
    /// After the notes changed: keeps the selected row if still listed.
    Refresh,
    /// After a notify or a deferred refresh: keeps the selection and waits nothing.
    Notify,
}

impl Run {
    pub(crate) fn is_user(self) -> bool {
        self == Self::Search
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
pub(crate) struct Started {
    pub(crate) id: RunId,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) run: Run,
    /// A keystroke supersedes a background run at once; it waits only behind a user search.
    pub(crate) behind_user: bool,
}

impl Drop for Runs {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Runs {
    /// Supersedes the run before: cancels its scan and takes the run a refresh set.
    pub(crate) fn start(&mut self, sync: &IndexSync) -> Started {
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
    pub(crate) fn scanning(&mut self, started: &Started) -> impl Future<Output = ()> + use<> {
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

    pub(crate) fn is_current(&self, id: RunId) -> bool {
        id == self.id
    }

    /// The picker dismisses: a deferred refresh has nothing left to refresh.
    pub(crate) fn confirmed(&mut self) {
        if let Some(scan) = &mut self.scan {
            scan.rerun = false;
        }
    }

    /// Lands a run that scans nothing: the delegate already set its rows.
    pub(crate) fn settle<D>(id: RunId, cx: &mut Context<Picker<D>>) -> Task<()>
    where
        D: PickerDelegate + AsMut<Self>,
    {
        cx.spawn(async move |picker, cx| {
            let _ = picker.update(cx, |picker, cx| {
                if picker.delegate.as_mut().is_current(id) {
                    picker.matches_updated(0, cx);
                }
            });
        })
    }

    /// The current run's rows landed; a refresh deferred behind it starts now.
    pub(crate) fn landed<D>(picker: &mut Picker<D>, cx: &mut Context<Picker<D>>)
    where
        D: PickerDelegate + AsMut<Self>,
    {
        let scan = picker.delegate.as_mut().scan.take();
        if scan.is_some_and(|scan| scan.rerun) {
            Self::refresh_or_defer(picker, Run::Notify, cx);
        }
    }

    /// Search spec D7: a refresh never cancels a running scan, whose query the user still waits on.
    pub(crate) fn refresh_or_defer<D>(picker: &mut Picker<D>, run: Run, cx: &mut Context<Picker<D>>)
    where
        D: PickerDelegate + AsMut<Self>,
    {
        let runs = picker.delegate.as_mut();
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
    pub(crate) fn observe<D>(sync: &Entity<IndexSync>, cx: &mut Context<Picker<D>>) -> Subscription
    where
        D: PickerDelegate + AsMut<Self>,
    {
        cx.observe(sync, |picker, sync, cx| {
            match picker.delegate.as_mut().watch.on_notify(sync.read(cx)) {
                Refresh::Skip => {}
                Refresh::Now => Self::refresh_or_defer(picker, Run::Notify, cx),
                Refresh::Later => {
                    let throttled = cx.spawn(async move |picker, cx| {
                        cx.background_executor().timer(BUILDING_REFRESH).await;
                        let _ = picker.update(cx, |picker, cx| {
                            Self::refresh_or_defer(picker, Run::Notify, cx);
                        });
                    });
                    picker.delegate.as_mut().watch.throttle(throttled);
                }
            }
        })
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
