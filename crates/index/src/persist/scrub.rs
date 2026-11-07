//! The scrub: a sample of the seeded notes re-parsed at the first `Ready`, so a parse that
//! drifted from the cache's rejects it (persisted index spec, P16).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use inkra_vault::{EntryId, Vault};

use crate::{IndexSnapshot, NoteContent};

/// How many seeded notes one launch re-parses.
pub const SCRUB: usize = 64;

/// `n` distinct indices below `len`, sorted, drawn by Floyd's algorithm from `crc`; reads no clock.
pub(crate) fn scrub_sample(crc: u32, len: usize, n: usize) -> Vec<usize> {
    let n = n.min(len);
    let mut random = SplitMix64(u64::from(crc));
    let mut picked = BTreeSet::new();
    for j in len - n..len {
        let at = random.below(j + 1);
        if !picked.insert(at) {
            picked.insert(j);
        }
    }
    picked.into_iter().collect()
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the remainder is below `m`, a usize"
    )]
    fn below(&mut self, m: usize) -> usize {
        (self.next() % m as u64) as usize
    }
}

/// The sampled notes and the index they were seeded into; load each path, then ask `mismatch`.
pub struct ScrubJob {
    picks: Vec<(EntryId, PathBuf)>,
    snapshot: IndexSnapshot,
}

impl ScrubJob {
    /// An id the vault no longer lists is dropped.
    pub(crate) fn new(ids: Vec<EntryId>, vault: &Vault, snapshot: IndexSnapshot) -> Self {
        let picks = ids
            .into_iter()
            .filter_map(|id| Some((id, vault.absolute(id)?)))
            .collect();
        Self { picks, snapshot }
    }

    /// The files to load, in the order `mismatch` takes their reads.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.picks.iter().map(|(_, path)| path.as_path())
    }

    /// Whether a note read at the stamp the index holds parses to other links or words. A failed
    /// read, a note the index holds no read for, or a file at another stamp proves nothing.
    ///
    /// # Panics
    ///
    /// If `loaded` does not hold one read per path, in `paths` order.
    #[must_use]
    pub fn mismatch(self, loaded: Vec<inkra_vault::Result<NoteContent>>) -> bool {
        assert_eq!(loaded.len(), self.picks.len(), "one read per pick");
        let ids: HashSet<EntryId> = self.picks.iter().map(|&(id, _)| id).collect();
        let held: HashMap<_, _> = self
            .snapshot
            .loaded()
            .filter(|(id, ..)| ids.contains(id))
            .map(|(id, _, read, source)| (id, (read.stamp, source)))
            .collect();
        let words = self.snapshot.words().held(&ids);
        self.picks.iter().zip(loaded).any(|(&(id, _), loaded)| {
            let (Ok(content), Some(&(stamp, source))) = (loaded, held.get(&id)) else {
                return false;
            };
            if stamp != content.read.stamp {
                return false;
            }
            let (held_words, length) = words
                .get(&id)
                .map_or((&[][..], 0), |(words, length)| (words.as_slice(), *length));
            *source != content.source
                || !held_words
                    .iter()
                    .map(|(word, count)| (&**word, *count))
                    .eq(content.words.iter())
                || length != content.words.length()
        })
    }
}
