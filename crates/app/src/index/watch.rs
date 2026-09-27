//! When an `IndexSync` observer refreshes: at once when built, throttled while loading.

use std::time::Duration;

use gpui::Task;

use super::IndexSync;

#[cfg(test)]
mod tests;

pub(crate) const BUILDING_REFRESH: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refresh {
    Skip,
    Now,
    /// After `BUILDING_REFRESH`; the owner hands the task to `throttle`.
    Later,
}

#[derive(Default)]
pub(crate) struct IndexWatch {
    /// The index revision and building state the owner last refreshed at.
    seen: Option<(u64, bool)>,
    throttled: Option<Task<()>>,
}

impl IndexWatch {
    pub(crate) fn on_notify(&self, sync: &IndexSync) -> Refresh {
        let building = sync.is_building();
        if self.seen == Some((sync.revision(), building)) {
            Refresh::Skip
        } else if !building {
            Refresh::Now
        } else if self.throttled.is_some() {
            Refresh::Skip
        } else {
            Refresh::Later
        }
    }

    pub(crate) fn seen(&mut self, sync: &IndexSync) {
        self.throttled = None;
        self.seen = Some((sync.revision(), sync.is_building()));
    }

    pub(crate) fn throttle(&mut self, task: Task<()>) {
        self.throttled = Some(task);
    }
}
