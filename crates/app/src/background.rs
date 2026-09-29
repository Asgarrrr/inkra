//! `inkra_vault::Jobs` on gpui's background threads: a walk's and a query's.

use gpui::{BackgroundExecutor, Priority};
use inkra_vault::{Job, Jobs};

pub(crate) struct Background {
    pub(crate) executor: BackgroundExecutor,
    /// A walk runs `Low`, as Zed's scanner, so a query's scans go first.
    pub(crate) priority: Priority,
}

impl Jobs for Background {
    fn parallelism(&self) -> usize {
        self.executor.num_cpus()
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(self.executor.scoped_priority(self.priority, move |scope| {
            for job in jobs {
                scope.spawn(job);
            }
        }))
    }
}
