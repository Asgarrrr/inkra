//! The seam a walk and a query run their jobs through.

use std::pin::Pin;

/// A job borrows from its caller: an adapter scopes it, never spawns it `'static`.
pub type Job<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Runs jobs to completion, a walk's or a query's: the app on its background threads, a test inline.
pub trait Jobs: Sync {
    /// How many jobs to start; a hint, a caller may start another number.
    fn parallelism(&self) -> usize;
    /// Resolves once every job in `jobs` has run to completion.
    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a>;
}

/// One job, run inline: `scan`, `rescan` and tests.
pub struct Serial;

impl Jobs for Serial {
    fn parallelism(&self) -> usize {
        1
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            for job in jobs {
                job.await;
            }
        })
    }
}
