//! What each decorations revision of a buffer changed, so a layout carries the lines it left alone
//! (incremental layout spec, D8).

use std::collections::VecDeque;
use std::ops::Range;

use inkra_text::{Bias, Buffer, ByteOffset, EditMark};

/// A layout further behind than this many bumps rebuilds the whole note.
const KEPT: usize = 64;

/// What one bump of a buffer's decorations revision changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Moved {
    /// Every mark followed the splices: no colour changed off the bytes they touched.
    Shift,
    /// Marks changed colour inside this range of the text at this edit mark.
    Range(EditMark, Range<ByteOffset>),
    /// Anything may have changed.
    Whole,
}

/// A buffer's decorations revision, and what each of its last bumps moved.
#[derive(Debug, Default)]
pub struct RevisionLog {
    /// The revision before the oldest bump kept.
    base: u64,
    bumps: VecDeque<(u64, Moved)>,
}

impl RevisionLog {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base: 0,
            bumps: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.bumps
            .back()
            .map_or(self.base, |&(revision, _)| revision)
    }

    /// Moves to `revision`, which `moved` describes.
    ///
    /// # Panics
    /// If `revision` is not past `revision()`.
    pub fn bump(&mut self, revision: u64, moved: Moved) {
        assert!(
            revision > self.revision(),
            "revision {revision} is not past {}",
            self.revision()
        );
        if self.bumps.len() == KEPT
            && let Some((oldest, _)) = self.bumps.pop_front()
        {
            self.base = oldest;
        }
        self.bumps.push_back((revision, moved));
    }

    /// The ranges of `buffer`'s text whose colours may differ from those at `revision`. `None` when
    /// anything may have changed: a `Whole` since, a range the buffer's log no longer follows, or
    /// a `revision` the log never held or no longer reaches.
    ///
    /// # Panics
    /// If a logged edit mark is past `buffer`'s: it came from another buffer.
    #[must_use]
    pub fn changed_since(&self, revision: u64, buffer: &Buffer) -> Option<Vec<Range<ByteOffset>>> {
        let after = if revision == self.base {
            0
        } else {
            self.bumps
                .iter()
                .position(|&(bumped, _)| bumped == revision)?
                + 1
        };
        self.bumps
            .range(after..)
            .filter_map(|(_, moved)| match moved {
                Moved::Shift => None,
                Moved::Range(mark, range) => Some(follow(buffer, *mark, range)),
                Moved::Whole => Some(None),
            })
            .collect()
    }
}

/// `range`, in the text at `mark`, in `buffer`'s text now; bytes typed at its edges join it.
fn follow(buffer: &Buffer, mark: EditMark, range: &Range<ByteOffset>) -> Option<Range<ByteOffset>> {
    let splices = buffer.edits_since(mark)?;
    Some(splices.fold(range.clone(), |range, splice| {
        splice.map(range.start, Bias::Left)..splice.map(range.end, Bias::Right)
    }))
}

#[cfg(test)]
mod tests;
