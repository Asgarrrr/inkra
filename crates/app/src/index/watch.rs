//! When an `IndexSync` observer refreshes: at once when built, throttled while loading.

use std::time::Duration;

use gpui::{Context, Entity, Subscription, Task};

use super::IndexSync;

#[cfg(test)]
mod tests;

pub(crate) const BUILDING_REFRESH: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refresh {
    Skip,
    Now,
    /// After `BUILDING_REFRESH`, through the task `throttle` holds.
    Later,
}

#[derive(Default)]
pub(crate) struct IndexWatch {
    /// The index revision and building state the owner last refreshed at.
    seen: Option<(u64, bool)>,
    throttled: Option<Task<()>>,
}

impl IndexWatch {
    /// `refresh` on `sync`'s notifies: at once when built, at most every `BUILDING_REFRESH`
    /// while loads land, never twice for one revision. `refresh` calls `seen`.
    pub(crate) fn observe<V: 'static>(
        sync: &Entity<IndexSync>,
        cx: &mut Context<V>,
        watch: fn(&mut V) -> &mut IndexWatch,
        refresh: fn(&mut V, &mut Context<V>),
    ) -> Subscription {
        cx.observe(sync, move |view, sync, cx| {
            match watch(view).on_notify(sync.read(cx)) {
                Refresh::Skip => {}
                Refresh::Now => refresh(view, cx),
                Refresh::Later => {
                    let throttled = cx.spawn(async move |view, cx| {
                        cx.background_executor().timer(BUILDING_REFRESH).await;
                        let _ = view.update(cx, refresh);
                    });
                    watch(view).throttle(throttled);
                }
            }
        })
    }

    fn on_notify(&self, sync: &IndexSync) -> Refresh {
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

    fn throttle(&mut self, task: Task<()>) {
        self.throttled = Some(task);
    }
}
