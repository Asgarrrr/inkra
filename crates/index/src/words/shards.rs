use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;

use foldhash::fast::FixedState;
use inkra_vault::EntryId;

use super::ordinal::Ordinal;

/// Maps per `Shards`, by the words bench over 16 and 256.
pub(super) const SHARDS: usize = 64;

/// The shard holding a key, the same in every snapshot.
pub(crate) trait ShardKey: Copy + Eq + Hash {
    fn shard(self) -> usize;
}

impl ShardKey for EntryId {
    fn shard(self) -> usize {
        // Fixed, not seeded per map: every snapshot must pick the same shard for a key.
        let hash = FixedState::default().hash_one(self);
        usize::from(hash.to_be_bytes()[0]) % SHARDS
    }
}

impl ShardKey for Ordinal {
    fn shard(self) -> usize {
        self.index() % SHARDS
    }
}

/// A map split into `SHARDS` maps behind `Arc`s, so a write under a held snapshot copies one.
#[derive(Debug, Clone)]
pub(crate) struct Shards<K, V> {
    maps: Arc<[Arc<HashMap<K, V>>]>,
    len: usize,
}

impl<K, V> Default for Shards<K, V> {
    /// One empty map shared by every slot, so a new `Shards` allocates one map, not `SHARDS`.
    fn default() -> Self {
        let empty = Arc::new(HashMap::new());
        Self {
            maps: (0..SHARDS).map(|_| Arc::clone(&empty)).collect(),
            len: 0,
        }
    }
}

impl<K: ShardKey, V: Clone> Shards<K, V> {
    pub(super) fn get(&self, key: K) -> Option<&V> {
        self.maps[key.shard()].get(&key)
    }

    fn contains_key(&self, key: K) -> bool {
        self.maps[key.shard()].contains_key(&key)
    }

    pub(super) fn insert(&mut self, key: K, value: V) {
        if self.shard_mut(key).insert(key, value).is_none() {
            self.len += 1;
        }
    }

    /// An absent key copies nothing.
    pub(super) fn remove(&mut self, key: K) -> Option<V> {
        if !self.contains_key(key) {
            return None;
        }
        let removed = self.shard_mut(key).remove(&key);
        self.len -= 1;
        removed
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.maps.iter().flat_map(|map| map.iter())
    }

    /// Copies the shard list and `key`'s shard only while a snapshot holds them.
    fn shard_mut(&mut self, key: K) -> &mut HashMap<K, V> {
        Arc::make_mut(&mut Arc::make_mut(&mut self.maps)[key.shard()])
    }

    /// Whether both hold the same shard list, so neither copied a shard since they split.
    #[cfg(test)]
    pub(super) fn is_shared(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.maps, &other.maps)
    }

    /// How many shards both still hold.
    #[cfg(test)]
    pub(super) fn shards_shared(&self, other: &Self) -> usize {
        self.maps
            .iter()
            .zip(other.maps.iter())
            .filter(|(mine, theirs)| Arc::ptr_eq(mine, theirs))
            .count()
    }
}

impl<K: ShardKey, V: Clone> FromIterator<(K, V)> for Shards<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Self {
        let mut shards = Self::default();
        for (key, value) in pairs {
            shards.insert(key, value);
        }
        shards
    }
}

#[cfg(test)]
impl<K: ShardKey, V: Clone> std::ops::Index<&K> for Shards<K, V> {
    type Output = V;

    fn index(&self, key: &K) -> &V {
        let Some(value) = self.get(*key) else {
            panic!("no such key");
        };
        value
    }
}

#[cfg(test)]
mod tests;
