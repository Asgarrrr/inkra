use std::collections::{BTreeMap, HashMap, HashSet};

use inkra_vault::EntryId;

use super::WordsSnapshot;
use crate::words::ordinal::Ordinal;

impl WordsSnapshot {
    /// Every visible `(word, note)` and its count, checking the store's invariants on the way.
    pub(crate) fn entries(&self) -> BTreeMap<(String, EntryId), u32> {
        let ids: HashMap<Ordinal, EntryId> =
            self.ordinals.iter().map(|(&id, &o)| (o, id)).collect();
        assert_eq!(ids.len(), self.ordinals.len(), "an ordinal names two notes");
        let visible = |ordinal: &Ordinal| {
            let Some(&id) = ids.get(ordinal) else {
                panic!("{ordinal:?} is visible but no note holds it");
            };
            id
        };
        let mut entries = BTreeMap::new();
        let mut held = HashSet::new();
        for (word, postings) in self.segment.words() {
            held.extend(postings.clone().map(|(ordinal, _)| ordinal));
            for (ordinal, count) in postings.filter(|&(ordinal, _)| self.alive.contains(ordinal)) {
                let seen = entries.insert((word.to_owned(), visible(&ordinal)), count);
                assert!(seen.is_none(), "{word:?} of {ordinal:?} is visible twice");
            }
        }
        assert!(
            self.alive.iter().all(|ordinal| held.contains(&ordinal)),
            "an alive bit names an ordinal the segment does not hold"
        );
        assert_eq!(held.len(), self.segment.notes());
        for (ordinal, words) in self.fresh.iter() {
            assert!(
                !self.alive.contains(*ordinal),
                "{ordinal:?} is alive and fresh"
            );
            assert!(!words.is_empty(), "{ordinal:?} is fresh with no words");
            for (word, count) in words.iter() {
                let seen = entries.insert((word.to_owned(), visible(ordinal)), count);
                assert!(seen.is_none(), "{word:?} of {ordinal:?} is visible twice");
            }
        }
        entries
    }
}
