use std::sync::Arc;
use std::time::{Duration, Instant};

use inkra_index::{MergeWhen, NoteContent, NoteIndex, NoteSet};

use crate::corpus::{Corpus, ENTRIES, NOTE_IDS, NOTES, contents};

/// Mirror `src/words/policy.rs`; `max_fresh` panics when they disagree.
pub(crate) const FRESH_MIN: usize = 256;
pub(crate) const FRESH_RATIO: usize = 8;

/// An index holding every note, none loaded.
pub(crate) fn loaded_notes() -> NoteIndex {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(ENTRIES.clone()));
    index
}

/// What one build cost: each insert, each merge's start, run and apply.
#[derive(Default)]
pub(crate) struct Build {
    inserts: Vec<Duration>,
    starts: Duration,
    runs: Vec<Duration>,
    applies: Vec<Duration>,
    /// The runs before the settle.
    due: usize,
}

impl Build {
    /// Inserts, merge starts and applies: what the foreground pays.
    pub(crate) fn foreground(&self) -> Duration {
        self.inserts.iter().sum::<Duration>() + self.starts + self.applies.iter().sum::<Duration>()
    }

    /// Starts, runs and applies merges while `when` starts one; the run is the background's part.
    pub(crate) fn merge(&mut self, index: &mut NoteIndex, when: MergeWhen) {
        loop {
            let started = Instant::now();
            let merge = index.words_merge(when);
            self.starts += started.elapsed();
            let Some(merge) = merge else {
                return;
            };
            let started = Instant::now();
            let merged = merge.run();
            self.runs.push(started.elapsed());
            let started = Instant::now();
            let retired = index.words_apply(merged);
            self.applies.push(started.elapsed());
            drop(retired);
        }
    }

    pub(crate) fn report(&self, corpus: Corpus) {
        let mut inserts = self.inserts.clone();
        inserts.sort_unstable();
        let at = |percent: usize| inserts[(inserts.len() - 1) * percent / 100];
        let max = |times: &[Duration]| times.iter().max().copied().unwrap_or_default();
        eprintln!(
            "build/50k notes, {}: foreground {:?} (inserts {:?}, merge starts {:?}, applies {:?}); \
             insert p50 {:?}, p99 {:?}, max {:?}; {} merges due then {} to settle, apply max {:?}; \
             background {:?}, largest merge {:?}, settle {:?}",
            corpus.name(),
            self.foreground(),
            self.inserts.iter().sum::<Duration>(),
            self.starts,
            self.applies.iter().sum::<Duration>(),
            at(50),
            at(99),
            max(&self.inserts),
            self.due,
            self.runs.len() - self.due,
            max(&self.applies),
            self.runs.iter().sum::<Duration>(),
            max(&self.runs),
            self.runs[self.due..].iter().sum::<Duration>(),
        );
    }
}

/// Inserts every note in vault order, merging when due and settling once loads end, as
/// `IndexSync` does; a merge runs inline, so no insert lands while it runs.
pub(crate) fn build(
    index: &mut NoteIndex,
    contents: impl IntoIterator<Item = NoteContent>,
) -> Build {
    let mut build = Build {
        inserts: Vec::with_capacity(NOTES),
        ..Build::default()
    };
    for (&id, content) in NOTE_IDS.iter().zip(contents) {
        let started = Instant::now();
        index.insert(id, content);
        build.inserts.push(started.elapsed());
        build.merge(index, MergeWhen::Due);
    }
    build.due = build.runs.len();
    build.merge(index, MergeWhen::Settle);
    build
}

pub(crate) fn settle(index: &mut NoteIndex) {
    Build::default().merge(index, MergeWhen::Settle);
}

/// The fresh tier at which the policy starts a merge at 50k notes; the segment holds the rest.
pub(crate) fn max_fresh(texts: &[Arc<str>]) -> NoteIndex {
    let fresh = FRESH_MIN.max(NOTES / FRESH_RATIO);
    let merged = NOTES - fresh;
    let mut index = loaded_notes();
    for (&id, content) in NOTE_IDS[..merged].iter().zip(contents(&texts[..merged])) {
        index.insert(id, content);
    }
    settle(&mut index);
    let mut due = None;
    let tail = NOTE_IDS[merged..].iter().zip(contents(&texts[merged..]));
    for (at, (&id, content)) in tail.enumerate() {
        index.insert(id, content);
        // Dropped unrun, so the tier stays fresh.
        if due.is_none() && index.words_merge(MergeWhen::Due).is_some() {
            due = Some(at + 1);
        }
    }
    assert!(
        due.is_some_and(|due| due.abs_diff(fresh) <= 2),
        "a merge was due at {due:?} fresh notes, the bench's policy mirror says {fresh}"
    );
    index
}
