use std::collections::{HashMap, HashSet};
use std::mem;
use std::sync::{Arc, Weak};

use inkra_vault::EntryId;

use super::ordinal::{Alive, Ordinal};
use super::policy::{self, MergeWhen};
use super::segment::Segment;
use super::shards::Shards;
use super::{NoteWords, wide};

/// What a fresh note costs beyond its words: its map slot, and its `Arc`'s counts and struct.
const FRESH_SLOT_BYTES: usize =
    size_of::<(Ordinal, Arc<NoteWords>)>() + 2 * size_of::<usize>() + size_of::<NoteWords>();

type Fresh = Shards<Ordinal, Arc<NoteWords>>;
pub(crate) type Ordinals = Shards<EntryId, Ordinal>;

/// Every note's folded words: one merged segment, the bits of its live notes, the notes set since.
#[derive(Default)]
pub(crate) struct Words {
    ordinals: Ordinals,
    segment: Arc<Segment>,
    /// An ordinal is here, in `fresh`, or in neither (a note with no words); never in both.
    alive: Arc<Alive>,
    fresh: Fresh,
    next: u32,
    /// The words of the live notes, repeats counted: the sum of their `NoteWords::len`.
    total: u64,
    /// The last merge started, until it applies.
    running: Option<Running>,
}

/// A merge started and not applied yet.
struct Running {
    /// Dead once the merge and its result are dropped: it will never apply.
    token: Weak<()>,
    /// The ordinals set or removed since it started.
    touched: HashSet<Ordinal>,
}

impl Words {
    /// Replaces `id`'s words; an emptied note keeps its ordinal, so autosave mints none.
    pub(crate) fn set(&mut self, id: EntryId, words: NoteWords) {
        let ordinal = match self.ordinals.get(id) {
            Some(&ordinal) => ordinal,
            None if words.is_empty() => return,
            None => {
                let ordinal = self.mint();
                self.ordinals.insert(id, ordinal);
                ordinal
            }
        };
        self.hide(ordinal);
        if !words.is_empty() {
            self.total += u64::from(words.length());
            self.fresh.insert(ordinal, Arc::new(words));
        }
    }

    /// Drops `id`'s words and frees its ordinal.
    pub(crate) fn remove(&mut self, id: EntryId) {
        let Some(ordinal) = self.ordinals.remove(id) else {
            return;
        };
        self.hide(ordinal);
    }

    /// One cleared bit or fresh entry per note, so a trash of thousands of notes is O(gone).
    pub(crate) fn remove_all(&mut self, gone: &HashSet<EntryId>) {
        for &id in gone {
            self.remove(id);
        }
    }

    pub(crate) fn snapshot(&self) -> WordsSnapshot {
        WordsSnapshot {
            ordinals: self.ordinals.clone(),
            segment: Arc::clone(&self.segment),
            alive: Arc::clone(&self.alive),
            fresh: self.fresh.clone(),
            next: self.next,
            total: self.total,
        }
    }

    /// Starts a merge when `when` asks for one and none runs; it captures only pointers.
    pub(crate) fn merge(&mut self, when: MergeWhen) -> Option<WordsMerge> {
        self.due(when).then(|| self.start())
    }

    fn due(&self, when: MergeWhen) -> bool {
        let fresh = self.fresh.len();
        let live = self.alive.len() + fresh;
        let due = policy::is_due(fresh, live, self.dead(), self.segment.notes());
        !self.is_merging() && (due || (when == MergeWhen::Settle && fresh > 0))
    }

    fn start(&mut self) -> WordsMerge {
        let token = Arc::new(());
        self.running = Some(Running {
            token: Arc::downgrade(&token),
            touched: HashSet::new(),
        });
        WordsMerge {
            token,
            segment: Arc::clone(&self.segment),
            alive: Arc::clone(&self.alive),
            fresh: self.fresh.clone(),
        }
    }

    /// Swaps in the running merge's result; writes since it started win, a foreign result returns.
    pub(crate) fn apply(&mut self, merged: MergedWords) -> RetiredWords {
        let token = Arc::downgrade(&merged.token);
        let running = self
            .running
            .take_if(|running| Weak::ptr_eq(&running.token, &token));
        let Some(Running { touched, .. }) = running else {
            self.forget_dead_merge();
            return RetiredWords {
                _held: Held::Foreign { _merged: merged },
            };
        };
        let MergedWords {
            segment, mut live, ..
        } = merged;
        for &ordinal in &touched {
            live.remove(ordinal);
        }
        let fresh: Fresh = touched
            .iter()
            .filter_map(|&ordinal| Some((ordinal, Arc::clone(self.fresh.get(ordinal)?))))
            .collect();
        RetiredWords {
            _held: Held::Replaced {
                _segment: mem::replace(&mut self.segment, Arc::new(segment)),
                _alive: mem::replace(&mut self.alive, Arc::new(live)),
                _fresh: mem::replace(&mut self.fresh, fresh),
            },
        }
    }

    pub(crate) fn has_fresh(&self) -> bool {
        !self.fresh.is_empty()
    }

    /// A merge dropped before it applied stops counting, so it cannot block every later one.
    pub(crate) fn is_merging(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| running.token.strong_count() > 0)
    }

    fn dead(&self) -> usize {
        self.segment.notes() - self.alive.len()
    }

    fn mint(&mut self) -> Ordinal {
        let ordinal = Ordinal::new(self.next);
        let Some(next) = self.next.checked_add(1) else {
            panic!("over 4 G notes were given words in one session");
        };
        self.next = next;
        ordinal
    }

    /// Clears `ordinal`'s bit and drops its fresh entry, copying each only when it held it.
    fn hide(&mut self, ordinal: Ordinal) {
        self.total -= u64::from(length(&self.segment, &self.alive, &self.fresh, ordinal));
        if self.alive.contains(ordinal) {
            Arc::make_mut(&mut self.alive).remove(ordinal);
        }
        self.fresh.remove(ordinal);
        self.forget_dead_merge();
        if let Some(running) = &mut self.running {
            running.touched.insert(ordinal);
        }
    }

    /// Else a dead merge's `touched` grows with every write until the next merge starts.
    fn forget_dead_merge(&mut self) {
        self.running
            .take_if(|running| running.token.strong_count() == 0);
    }

    /// A store holding `segment` merged, its `alive` ordinals each named by one of `ordinals`.
    ///
    /// # Panics
    ///
    /// If the parts break an invariant the store keeps.
    pub(crate) fn restore(
        segment: Segment,
        alive: Alive,
        ordinals: Ordinals,
        next: u32,
        total: u64,
    ) -> Self {
        let named: Alive = ordinals.iter().map(|(_, &ordinal)| ordinal).collect();
        assert!(
            named.len() == ordinals.len() && named.len() == alive.len(),
            "each alive ordinal names one note"
        );
        assert!(
            alive
                .iter()
                .all(|ordinal| named.contains(ordinal) && ordinal.get() < next),
            "an alive ordinal names no note or is not minted"
        );
        assert!(alive.len() <= segment.notes(), "more alive than held");
        let live: u64 = alive
            .iter()
            .map(|ordinal| u64::from(segment.length(ordinal)))
            .sum();
        assert_eq!(total, live, "total is the sum of the live lengths");
        Self {
            ordinals,
            segment: Arc::new(segment),
            alive: Arc::new(alive),
            fresh: Fresh::default(),
            next,
            total,
            running: None,
        }
    }

    /// Each note's words while none is merged: what a seed puts back over the cache's.
    ///
    /// # Panics
    ///
    /// If a merge applied: its notes' words are no longer apart.
    pub(crate) fn into_fresh(self) -> HashMap<EntryId, NoteWords> {
        assert_eq!(self.segment.notes(), 0, "no merge applies before the seed");
        self.ordinals
            .iter()
            .filter_map(|(&id, &ordinal)| Some((id, NoteWords::clone(self.fresh.get(ordinal)?))))
            .collect()
    }

    /// Starts a merge, due or not, unless one runs or there is nothing to do.
    #[cfg(test)]
    pub(crate) fn start_merge(&mut self) -> Option<WordsMerge> {
        let work = !self.fresh.is_empty() || self.dead() > 0;
        (work && !self.is_merging()).then(|| self.start())
    }

    /// Starts, runs and applies merges until none is left to do; a running merge blocks it.
    #[cfg(test)]
    pub(crate) fn merge_all(&mut self) {
        while let Some(merge) = self.start_merge() {
            let _ = self.apply(merge.run());
        }
    }
}

/// A merge of the fresh notes as they stood when it started; run it off the foreground.
pub struct WordsMerge {
    token: Arc<()>,
    segment: Arc<Segment>,
    alive: Arc<Alive>,
    fresh: Fresh,
}

impl WordsMerge {
    /// Decodes the segment, merges the fresh words in, and encodes the result.
    #[must_use]
    pub fn run(self) -> MergedWords {
        let (segment, live) = merge(&self.segment, &self.alive, &self.fresh);
        MergedWords {
            token: self.token,
            segment,
            live,
        }
    }
}

/// `segment`'s `alive` postings plus `fresh`'s, and the ordinals the result holds.
fn merge(segment: &Segment, alive: &Alive, fresh: &Fresh) -> (Segment, Alive) {
    let mut fresh: Vec<(Ordinal, &NoteWords)> = fresh
        .iter()
        .map(|(&ordinal, words)| (ordinal, &**words))
        .collect();
    fresh.sort_unstable_by_key(|&(ordinal, _)| ordinal);
    let segment = segment.merge(alive, &fresh);
    let live = alive
        .iter()
        .chain(fresh.iter().map(|&(ordinal, _)| ordinal))
        .collect();
    (segment, live)
}

/// A merge's result, to swap in on the foreground.
pub struct MergedWords {
    /// Equals the store's running token only while the store holds the segment this merge read.
    token: Arc<()>,
    segment: Segment,
    /// The ordinals `segment` holds postings for.
    live: Alive,
}

/// What an apply let go of; freeing a segment takes milliseconds, so drop it off the foreground.
#[must_use]
pub struct RetiredWords {
    _held: Held,
}

/// Held only to be dropped.
enum Held {
    /// The segment, bits and fresh notes an applied merge replaced.
    Replaced {
        _segment: Arc<Segment>,
        _alive: Arc<Alive>,
        _fresh: Fresh,
    },
    /// Another store's result, never applied.
    Foreign { _merged: MergedWords },
}

/// The words of `ordinal`'s note, repeats counted: from the segment while alive, else fresh.
fn length(segment: &Segment, alive: &Alive, fresh: &Fresh, ordinal: Ordinal) -> u32 {
    if alive.contains(ordinal) {
        segment.length(ordinal)
    } else {
        fresh.get(ordinal).map_or(0, |words| words.length())
    }
}

/// The word index at one moment. Cloning is four `Arc` bumps.
#[derive(Clone)]
pub struct WordsSnapshot {
    pub(super) ordinals: Ordinals,
    pub(super) segment: Arc<Segment>,
    pub(super) alive: Arc<Alive>,
    pub(super) fresh: Fresh,
    pub(crate) next: u32,
    pub(crate) total: u64,
}

impl WordsSnapshot {
    /// A structural estimate, without allocator overhead; a shared segment counts in each snapshot.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let fresh: usize = self
            .fresh
            .iter()
            .map(|(_, words)| FRESH_SLOT_BYTES + words.heap_bytes())
            .sum();
        self.segment.heap_bytes()
            + self.alive.heap_bytes()
            + fresh
            + self.ordinals.len() * size_of::<(EntryId, Ordinal)>()
    }

    pub(super) fn length(&self, ordinal: Ordinal) -> u32 {
        length(&self.segment, &self.alive, &self.fresh, ordinal)
    }

    /// The note's ordinal; `None` when it never had words.
    pub(crate) fn ordinal(&self, id: EntryId) -> Option<Ordinal> {
        self.ordinals.get(id).copied()
    }

    /// The segment with the fresh notes merged in, and the ordinals it holds.
    pub(crate) fn merged(&self) -> (Segment, Alive) {
        merge(&self.segment, &self.alive, &self.fresh)
    }

    /// One past the last ordinal minted, a removed note's included; for tests and the bench.
    #[doc(hidden)]
    #[must_use]
    pub fn ordinal_span(&self) -> usize {
        wide(self.next)
    }
}

mod held;

#[cfg(test)]
mod entries;
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod tests;
