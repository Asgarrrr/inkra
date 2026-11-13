//! A release-only log of when each step of a launch happened, since `run()` started.

use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, Global};

/// Runs one foreground turn and counts its time (`Timeline::turn`).
pub(crate) fn timed<R>(cx: &mut AsyncApp, turn: impl FnOnce(&mut AsyncApp) -> R) -> R {
    let started = Instant::now();
    let result = turn(cx);
    let elapsed = started.elapsed();
    cx.update(|cx| Timeline::turn(elapsed, cx));
    result
}

#[derive(Clone, Copy)]
pub(crate) enum Step {
    FirstFrame,
    /// The root level applied: the sidebar shows it.
    Root,
    /// The vault turned scanned, also after a failed full pass (O8).
    Scanned,
    /// The first build applied: quick open lists every note.
    NamesReady,
    /// The index cache's outcome landed, by `Outcome::name`.
    Cached(&'static str),
    /// Nothing left to load: search sees every note.
    Indexed {
        notes: usize,
        corpus_bytes: usize,
        words_bytes: usize,
        names_bytes: usize,
    },
    /// The shell's launch task ran: the registry activates its `after_first_frame` features.
    Registry,
    /// A feature activated, by id.
    Activated(&'static str),
}

struct Indexed {
    at: Duration,
    notes: usize,
    corpus_bytes: usize,
    words_bytes: usize,
    names_bytes: usize,
}

pub(crate) struct Timeline {
    started: Instant,
    first_frame: Option<Duration>,
    /// A span, not a time since `started`: the read blocks the launch before the first frame.
    session_read: Option<Duration>,
    root: Option<Duration>,
    scanned: Option<Duration>,
    names_ready: Option<Duration>,
    cache: Option<(&'static str, Duration)>,
    /// A span: the foreground turn that applied the cache's seed.
    seed_turn: Option<Duration>,
    indexed: Option<Indexed>,
    longest_turn: Duration,
    registry: Option<Duration>,
    /// The first activation of each feature, in order.
    activated: Vec<(&'static str, Duration)>,
}

impl Global for Timeline {}

impl Timeline {
    pub(crate) fn new(started: Instant) -> Self {
        Self {
            started,
            first_frame: None,
            session_read: None,
            root: None,
            scanned: None,
            names_ready: None,
            cache: None,
            seed_turn: None,
            indexed: None,
            longest_turn: Duration::ZERO,
            registry: None,
            activated: Vec::new(),
        }
    }

    /// Does nothing when no launch set a timeline, as in tests.
    pub(crate) fn mark(step: Step, cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let timeline = cx.global_mut::<Self>();
        if timeline.record(step, Instant::now()) && cfg!(not(debug_assertions)) {
            eprintln!("{}", timeline.line());
        }
    }

    /// Ends the launch's log: a later vault's marks would read as this launch's.
    pub(crate) fn end(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.remove_global::<Self>();
        }
    }

    /// A foreground turn of the launch's work; the longest one before the line is due is logged.
    pub(crate) fn turn(turn: Duration, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().record_turn(turn);
        }
    }

    /// How long the session file's read held the launch; does nothing without a timeline.
    pub(crate) fn session_read(took: Duration, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().record_session_read(took);
        }
    }

    /// How long the turn that applied the cache's seed took; does nothing without a timeline.
    pub(crate) fn seed_turn(took: Duration, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().record_seed_turn(took);
        }
    }

    fn record_session_read(&mut self, took: Duration) {
        _ = self.session_read.get_or_insert(took);
    }

    fn record_seed_turn(&mut self, took: Duration) {
        _ = self.seed_turn.get_or_insert(took);
    }

    fn record_turn(&mut self, turn: Duration) {
        if !self.is_complete() {
            self.longest_turn = self.longest_turn.max(turn);
        }
    }

    #[cfg(test)]
    pub(crate) fn longest_turn(&self) -> Duration {
        self.longest_turn
    }

    /// True for the step that completes both `Indexed` and `FirstFrame`: the line is due.
    fn record(&mut self, step: Step, now: Instant) -> bool {
        let was_complete = self.is_complete();
        let at = now.saturating_duration_since(self.started);
        match step {
            Step::FirstFrame => _ = self.first_frame.get_or_insert(at),
            Step::Root => _ = self.root.get_or_insert(at),
            Step::Scanned => _ = self.scanned.get_or_insert(at),
            Step::NamesReady => _ = self.names_ready.get_or_insert(at),
            Step::Cached(name) => _ = self.cache.get_or_insert((name, at)),
            Step::Indexed {
                notes,
                corpus_bytes,
                words_bytes,
                names_bytes,
            } => {
                self.indexed.get_or_insert(Indexed {
                    at,
                    notes,
                    corpus_bytes,
                    words_bytes,
                    names_bytes,
                });
            }
            Step::Registry => _ = self.registry.get_or_insert(at),
            Step::Activated(id) => {
                if !self.activated.iter().any(|&(seen, _)| seen == id) {
                    self.activated.push((id, at));
                }
            }
        }
        !was_complete && self.is_complete()
    }

    /// Root, scanned and names ready precede `Indexed`, so waiting on them adds nothing but a way to never log.
    fn is_complete(&self) -> bool {
        self.first_frame.is_some() && self.indexed.is_some()
    }

    pub(crate) fn line(&self) -> String {
        let at =
            |step: Option<Duration>| step.map_or_else(|| "-".to_owned(), |at| format!("{at:.1?}"));
        let indexed = self.indexed.as_ref().map_or_else(
            || "-".to_owned(),
            |indexed| {
                // Two decimals: the startup budget is judged on this figure.
                format!(
                    "{:.2?} ({} notes, corpus {} bytes, words {} bytes, names {} bytes, longest turn {:.1?})",
                    indexed.at,
                    indexed.notes,
                    indexed.corpus_bytes,
                    indexed.words_bytes,
                    indexed.names_bytes,
                    self.longest_turn
                )
            },
        );
        let span = |took: Option<Duration>| {
            took.map_or_else(|| "-".to_owned(), |took| format!("{took:.2?}"))
        };
        let cache = self
            .cache
            .map_or_else(|| "-".to_owned(), |(name, at)| format!("{name} {at:.1?}"));
        let activated = if self.activated.is_empty() {
            "-".to_owned()
        } else {
            let each: Vec<_> = self
                .activated
                .iter()
                .map(|(id, at)| format!("{id} {at:.1?}"))
                .collect();
            each.join("; ")
        };
        format!(
            "launch: first frame {}, session read {}, root {}, scanned {}, names ready {}, \
             cache {cache}, seed turn {}, indexed {indexed}, registry {}, activated {activated}",
            at(self.first_frame),
            span(self.session_read),
            at(self.root),
            at(self.scanned),
            at(self.names_ready),
            span(self.seed_turn),
            at(self.registry)
        )
    }
}

#[cfg(test)]
mod tests;
