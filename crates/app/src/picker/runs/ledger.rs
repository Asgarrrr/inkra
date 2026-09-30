//! Which run is current, which scan is in flight, and what follows it. No gpui.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(test)]
mod tests;

/// Waited only behind a user search, so an idle or refreshing picker answers at once.
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);
/// After the names change, as Zed's file finder waits after a worktree update.
pub(crate) const NOTES_CHANGED_REFRESH: Duration = Duration::from_millis(200);

/// A scan in flight. Search spec D7: a refresh never cancels it; it marks it for a re-run instead.
struct Scan {
    /// Only behind a user run does a keystroke wait.
    user: bool,
    /// A refresh arrived during the scan: one re-run follows once it applies.
    rerun: bool,
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

#[derive(Default)]
pub(crate) struct Ledger {
    /// Set by the next run and on drop: a scan already running on another thread ignores a dropped task.
    cancel: Arc<AtomicBool>,
    id: RunId,
    scan: Option<Scan>,
    /// Set by a refresh for the next run only.
    next: Run,
}

/// The run `Ledger::start` begins.
#[must_use]
pub(crate) struct Started {
    pub(crate) id: RunId,
    pub(crate) run: Run,
    /// Before the scan: `SEARCH_DEBOUNCE` behind a user scan, `NOTES_CHANGED_REFRESH`, or none.
    pub(crate) wait: Option<Duration>,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Drop for Ledger {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Ledger {
    /// Supersedes the run before: sets its cancel flag, takes the run a refresh set.
    pub(crate) fn start(&mut self) -> Started {
        std::mem::take(&mut self.cancel).store(true, Ordering::Relaxed);
        self.id = RunId(self.id.0 + 1);
        let run = std::mem::take(&mut self.next);
        // This run reads the newest index, whatever made the scan before it stale.
        let behind_user = std::mem::take(&mut self.scan).is_some_and(|scan| scan.user);
        Started {
            id: self.id,
            run,
            wait: run.wait(behind_user),
            cancel: Arc::clone(&self.cancel),
        }
    }

    /// Refreshes defer behind the started run until it lands.
    pub(crate) fn begin_scan(&mut self, started: &Started) {
        debug_assert_eq!(started.id, self.id, "begin_scan on a superseded run");
        self.scan = Some(Scan {
            user: started.run.is_user(),
            rerun: false,
        });
    }

    pub(crate) fn is_current(&self, id: RunId) -> bool {
        id == self.id
    }

    /// Search spec D7: `None` while a scan is in flight, which re-runs once it lands;
    /// else `run`, which the next `start` takes.
    pub(crate) fn refresh(&mut self, run: Run) -> Option<Run> {
        if let Some(scan) = &mut self.scan {
            scan.rerun = true;
            None
        } else {
            self.next = run;
            Some(run)
        }
    }

    /// The current run's rows landed: ends its scan, returns the re-run a refresh deferred.
    pub(crate) fn landed(&mut self) -> Option<Run> {
        self.scan
            .take()
            .filter(|scan| scan.rerun)
            .map(|_| Run::Notify)
    }
}
