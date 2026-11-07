//! When the cache file is due a write: at the startup's end, then after a quiet window (persisted
//! index spec, P14). The caller passes the clock.

use std::mem;
use std::time::{Duration, Instant};

/// How long the index stays unchanged before a write.
pub const QUIET: Duration = Duration::from_mins(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Now,
    /// Ask again at this instant, unless the revision moves first.
    At(Instant),
    Idle,
}

/// The revision the file holds and the one waiting out the quiet window.
pub struct Writes {
    written: Option<u64>,
    /// The next call is the startup's end, which writes without waiting.
    first: bool,
    /// The revision last seen and when it was first seen.
    seen: Option<(u64, Instant)>,
}

impl Writes {
    /// `written`: the revision the file on disk holds, `None` when it holds none.
    #[must_use]
    pub fn new(written: Option<u64>) -> Self {
        Self {
            written,
            first: true,
            seen: None,
        }
    }

    /// Whether `revision` is due a write at `now`; only a `ready` index is written.
    pub fn due(&mut self, revision: u64, ready: bool, now: Instant) -> Due {
        let first = mem::take(&mut self.first);
        if !ready || self.written == Some(revision) {
            return Due::Idle;
        }
        if first {
            return Due::Now;
        }
        let since = match self.seen {
            Some((seen, since)) if seen == revision => since,
            _ => {
                self.seen = Some((revision, now));
                now
            }
        };
        let at = since + QUIET;
        if now >= at { Due::Now } else { Due::At(at) }
    }

    /// A write of `revision` started.
    pub fn started(&mut self, revision: u64) {
        self.written = Some(revision);
    }
}
