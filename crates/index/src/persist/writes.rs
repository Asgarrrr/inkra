//! When the cache file is due a write: at the startup's end, then after a quiet window or 500
//! changes, backing off after a failure (persisted index spec, P14). The caller passes the clock.

use std::mem;
use std::time::{Duration, Instant};

/// How long the index stays unchanged before a write.
pub const QUIET: Duration = Duration::from_mins(1);
/// How many changes since the last write start one without waiting.
pub const CHANGES: u64 = 500;
/// The longest a failed write waits before the next try.
pub const MAX_BACKOFF: Duration = Duration::from_mins(10);

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
    /// The revision `CHANGES` counts from.
    base: u64,
    /// The last failure's delay and when the next try is due.
    backoff: Option<(Duration, Instant)>,
}

impl Writes {
    /// `written`: the revision the file on disk holds, `None` when it holds none.
    #[must_use]
    pub fn new(written: Option<u64>) -> Self {
        Self {
            written,
            first: true,
            seen: None,
            base: written.unwrap_or(0),
            backoff: None,
        }
    }

    /// Whether `revision` is due a write at `now`; only a `ready` index is written.
    pub fn due(&mut self, revision: u64, ready: bool, now: Instant) -> Due {
        let first = mem::take(&mut self.first);
        if !ready || self.written == Some(revision) {
            return Due::Idle;
        }
        if let Some((_, retry)) = self.backoff {
            return if now < retry {
                Due::At(retry)
            } else {
                Due::Now
            };
        }
        if first || revision.saturating_sub(self.base) >= CHANGES {
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
        self.base = revision;
    }

    /// The write of `revision` failed at `now`: the old file stays, and the next try waits.
    pub fn failed(&mut self, revision: u64, now: Instant) {
        if self.written == Some(revision) {
            self.written = None;
        }
        let delay = self
            .backoff
            .map_or(QUIET, |(delay, _)| (delay * 2).min(MAX_BACKOFF));
        self.backoff = Some((delay, now + delay));
    }

    /// A write landed.
    pub fn wrote(&mut self) {
        self.backoff = None;
    }
}
