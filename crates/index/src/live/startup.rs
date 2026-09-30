//! When the words merge, and the one turn the startup load ends in (L7).

use super::LiveIndex;
use crate::words::{MergeWhen, MergedWords, RetiredWords, WordsMerge};

/// The startup load's progress toward its `Indexed` report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Startup {
    #[default]
    Loading,
    /// The load ended; a settle merge runs, or is asked for every turn until one starts.
    Settling,
    /// The settle applied, or found nothing to merge, within this turn.
    Settled,
    /// `Indexed` was reported.
    Done,
}

/// A word merge to run off the foreground, then hand to `LiveIndex::merged`.
#[must_use = "run it off the foreground"]
pub struct Merge {
    words: WordsMerge,
    settle: bool,
}

pub struct Merged {
    words: MergedWords,
    settle: bool,
}

pub(super) struct TurnEnd {
    pub(super) merge: Option<Merge>,
    /// True once: the startup load ended and its settle applied.
    pub(super) indexed: bool,
}

impl Merge {
    #[must_use]
    pub fn run(self) -> Merged {
        Merged {
            words: self.words.run(),
            settle: self.settle,
        }
    }
}

impl LiveIndex {
    /// After every foreground turn: steps the startup, and starts a due merge or the settle.
    #[must_use]
    pub(super) fn end_turn(&mut self) -> TurnEnd {
        // `is_building` changes only inside a turn, so this is the turn the load ended in.
        if self.startup == Startup::Loading && !self.is_building() {
            self.startup = Startup::Settling;
        }
        let settle = self.startup == Startup::Settling;
        let when = if settle {
            MergeWhen::Settle
        } else {
            MergeWhen::Due
        };
        let merge = self
            .index
            .words_merge(when)
            .map(|words| Merge { words, settle });
        // Else a merge runs, and the settle starts once it applies.
        if merge.is_none() && settle && self.index.words_settled() {
            self.startup = Startup::Settled;
        }
        let mut indexed = false;
        if self.startup == Startup::Settled {
            if self.is_building() {
                self.startup = Startup::Loading;
            } else {
                self.startup = Startup::Done;
                indexed = true;
            }
        }
        TurnEnd { merge, indexed }
    }

    /// Swaps a merge in; a settle's apply ends the settle. Changes no answer, so notifies no reader.
    #[must_use = "free the retired words off the foreground"]
    pub(super) fn merged(&mut self, merged: Merged) -> RetiredWords {
        if merged.settle {
            self.startup = Startup::Settled;
        }
        self.index.words_apply(merged.words)
    }
}
