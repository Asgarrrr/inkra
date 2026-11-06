//! ⌘⇧F's read texts, kept while their stamp is the snapshot's and the budget allows.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use inkra_vault::{EntryId, Stamp};

pub const TEXT_CACHE_BYTES: usize = 8 << 20;

struct Cached {
    text: Arc<str>,
    stamp: Stamp,
    tick: u64,
}

/// An LRU of note texts by bytes; a pinned text is never evicted and still counts.
pub struct TextCache {
    budget: usize,
    used: usize,
    entries: HashMap<EntryId, Cached>,
    /// Oldest first.
    recency: BTreeMap<u64, EntryId>,
    next_tick: u64,
    pinned: HashSet<EntryId>,
}

impl TextCache {
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            used: 0,
            entries: HashMap::new(),
            recency: BTreeMap::new(),
            next_tick: 0,
            pinned: HashSet::new(),
        }
    }

    /// The text at `stamp`; a text at any other stamp is dropped.
    pub fn get(&mut self, id: EntryId, stamp: Stamp) -> Option<Arc<str>> {
        let cached = self.entries.get_mut(&id)?;
        if cached.stamp != stamp {
            self.remove(id);
            return None;
        }
        self.recency.remove(&cached.tick);
        cached.tick = self.next_tick;
        self.next_tick += 1;
        self.recency.insert(cached.tick, id);
        Some(Arc::clone(&cached.text))
    }

    pub fn insert(&mut self, id: EntryId, text: Arc<str>, stamp: Stamp) {
        self.remove(id);
        let tick = self.next_tick;
        self.next_tick += 1;
        self.used += text.len();
        self.recency.insert(tick, id);
        self.entries.insert(id, Cached { text, stamp, tick });
        self.evict();
    }

    /// Pins `ids` alone.
    pub fn pin_only(&mut self, ids: &HashSet<EntryId>) {
        self.pinned.clone_from(ids);
        self.evict();
    }

    fn remove(&mut self, id: EntryId) {
        if let Some(cached) = self.entries.remove(&id) {
            self.recency.remove(&cached.tick);
            self.used -= cached.text.len();
        }
    }

    fn evict(&mut self) {
        while self.used > self.budget {
            let Some(&oldest) = self.recency.values().find(|id| !self.pinned.contains(id)) else {
                return;
            };
            self.remove(oldest);
        }
    }
}
