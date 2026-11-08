use std::collections::{HashMap, HashSet};

use inkra_vault::EntryId;

use crate::key::Key;
use crate::source::Source;

/// A note link's stem key → (source, index into its links). Every link resolving to `T`
/// has `T`'s stem, so this finds backlinks without a reverse map. No row is empty.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Stems(HashMap<Key, Vec<(EntryId, usize)>>);

impl Stems {
    /// Adds a row per note link of `source`; `id` holds none beforehand.
    pub(crate) fn insert(&mut self, id: EntryId, source: &Source) {
        for (at, key) in source.note_stems() {
            self.0.entry(key).or_default().push((id, at));
        }
    }

    /// Drops the rows `insert(id, source)` added.
    pub(crate) fn remove(&mut self, id: EntryId, source: &Source) {
        for (_, key) in source.note_stems() {
            self.retain(&key, |source| source != id);
        }
    }

    /// Drops the rows of every `gone` note, `sources` being theirs.
    pub(crate) fn remove_all<'a>(
        &mut self,
        gone: &HashSet<EntryId>,
        sources: impl IntoIterator<Item = &'a Source>,
    ) {
        let keys: HashSet<Key> = sources
            .into_iter()
            .flat_map(|source| source.note_stems().map(|(_, key)| key))
            .collect();
        // Removing one note at a time rescans a shared row like `index` once per gone note.
        for key in keys {
            self.retain(&key, |source| !gone.contains(&source));
        }
    }

    pub(crate) fn get(&self, stem: &Key) -> Option<&[(EntryId, usize)]> {
        self.0.get(stem).map(Vec::as_slice)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn values(&self) -> impl Iterator<Item = &Vec<(EntryId, usize)>> {
        self.0.values()
    }

    fn retain(&mut self, key: &Key, keep: impl Fn(EntryId) -> bool) {
        if let Some(rows) = self.0.get_mut(key) {
            rows.retain(|&(source, _)| keep(source));
            if rows.is_empty() {
                self.0.remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests;
