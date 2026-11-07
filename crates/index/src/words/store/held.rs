use std::collections::{HashMap, HashSet};

use inkra_vault::EntryId;

use super::WordsSnapshot;

/// A note's words, sorted, with their counts, and its length.
pub(crate) type Held = (Vec<(Box<str>, u32)>, u32);

impl WordsSnapshot {
    /// Each of `ids`' words, sorted, and its length, in one pass over the segment; an id with
    /// no words is absent.
    pub(crate) fn held(&self, ids: &HashSet<EntryId>) -> HashMap<EntryId, Held> {
        let mut held = HashMap::with_capacity(ids.len());
        let mut alive = HashMap::new();
        for &id in ids {
            let Some(ordinal) = self.ordinal(id) else {
                continue;
            };
            if self.alive.contains(ordinal) {
                alive.insert(ordinal, id);
                held.insert(id, (Vec::new(), self.segment.length(ordinal)));
            } else if let Some(words) = self.fresh.get(ordinal) {
                let fresh = words.iter().map(|(word, count)| (word.into(), count));
                held.insert(id, (fresh.collect(), words.length()));
            }
        }
        for (word, postings) in self.segment.words() {
            for (ordinal, count) in postings {
                if let Some((words, _)) = alive.get(&ordinal).and_then(|id| held.get_mut(id)) {
                    words.push((word.into(), count));
                }
            }
        }
        held
    }
}
