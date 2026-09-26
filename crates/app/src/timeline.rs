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
    /// Nothing left to load: search sees every note.
    Indexed {
        notes: usize,
        corpus_bytes: usize,
        words_bytes: usize,
    },
}

struct Indexed {
    at: Duration,
    notes: usize,
    corpus_bytes: usize,
    words_bytes: usize,
}

pub(crate) struct Timeline {
    started: Instant,
    first_frame: Option<Duration>,
    root: Option<Duration>,
    scanned: Option<Duration>,
    names_ready: Option<Duration>,
    indexed: Option<Indexed>,
    longest_turn: Duration,
}

impl Global for Timeline {}

impl Timeline {
    pub(crate) fn new(started: Instant) -> Self {
        Self {
            started,
            first_frame: None,
            root: None,
            scanned: None,
            names_ready: None,
            indexed: None,
            longest_turn: Duration::ZERO,
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

    /// A foreground turn of the launch's work; the longest one before the line is due is logged.
    pub(crate) fn turn(turn: Duration, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().record_turn(turn);
        }
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
            Step::Indexed {
                notes,
                corpus_bytes,
                words_bytes,
            } => {
                self.indexed.get_or_insert(Indexed {
                    at,
                    notes,
                    corpus_bytes,
                    words_bytes,
                });
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
                    "{:.2?} ({} notes, corpus {} bytes, words {} bytes, longest turn {:.1?})",
                    indexed.at,
                    indexed.notes,
                    indexed.corpus_bytes,
                    indexed.words_bytes,
                    self.longest_turn
                )
            },
        );
        format!(
            "launch: first frame {}, root {}, scanned {}, names ready {}, indexed {indexed}",
            at(self.first_frame),
            at(self.root),
            at(self.scanned),
            at(self.names_ready)
        )
    }
}

#[cfg(test)]
mod tests;
