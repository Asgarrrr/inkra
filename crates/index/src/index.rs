use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;

use inkra_markdown::Link;
use inkra_vault::EntryId;

use crate::key::Key;
use crate::names::NamesSnapshot;
use crate::note_set::NoteSet;
use crate::notes::{NoteTable, Query, Resolution};
use crate::source::{Source, SourceLink};
use crate::target::{Style, Target};

/// Every note's links, resolved on demand against the current notes, so a create, rename or
/// trash invalidates nothing.
#[derive(Default)]
pub struct NoteIndex {
    notes: NoteTable,
    names: NamesSnapshot,
    sources: HashMap<EntryId, Source>,
    /// A note link's stem key → (source, index into its links). Every link resolving to `T`
    /// has `T`'s stem, so this finds backlinks without a reverse map.
    by_stem: HashMap<Key, Vec<(EntryId, usize)>>,
}

/// The links of one source that resolve to a note, in document order.
#[derive(Debug, PartialEq, Eq)]
pub struct Backlink<'a> {
    pub source: EntryId,
    pub links: Vec<&'a SourceLink>,
}

impl NoteIndex {
    /// Replaces the notes links resolve against and quick open lists, and drops the source of
    /// every note gone. The caller frees the previous set, so a 50k table need not be freed on
    /// the foreground.
    #[must_use]
    pub fn set_notes(&mut self, notes: NoteSet) -> NoteSet {
        let previous = NoteSet {
            notes: mem::replace(&mut self.notes, notes.notes),
            names: mem::replace(&mut self.names, notes.names),
        };
        let notes = &self.notes;
        let mut gone = HashSet::new();
        let mut stems = HashSet::new();
        self.sources.retain(|id, source| {
            if notes.contains(*id) {
                return true;
            }
            gone.insert(*id);
            for link in &source.links {
                if let Target::Note { path, .. } = &link.target {
                    stems.insert(Key::stem(path));
                }
            }
            false
        });
        // Removing one note at a time rescans a shared row like `index` once per gone note.
        for key in stems {
            if let Some(rows) = self.by_stem.get_mut(&key) {
                rows.retain(|(source, _)| !gone.contains(source));
                if rows.is_empty() {
                    self.by_stem.remove(&key);
                }
            }
        }
        previous
    }

    /// The quick-open candidates of the last `set_notes`. Cloning is an `Arc` bump.
    #[must_use]
    pub fn names(&self) -> NamesSnapshot {
        self.names.clone()
    }

    /// Replaces `id`'s links. Ignored for an id that is not a note, so a load finishing after
    /// its note was trashed leaves nothing behind.
    pub fn insert(&mut self, id: EntryId, source: Source) {
        if !self.notes.contains(id) {
            return;
        }
        self.remove(id);
        for (at, link) in source.links.iter().enumerate() {
            if let Target::Note { path, .. } = &link.target {
                self.by_stem
                    .entry(Key::stem(path))
                    .or_default()
                    .push((id, at));
            }
        }
        self.sources.insert(id, source);
    }

    pub fn remove(&mut self, id: EntryId) {
        let Some(old) = self.sources.remove(&id) else {
            return;
        };
        for link in &old.links {
            if let Target::Note { path, .. } = &link.target {
                let key = Key::stem(path);
                if let Some(rows) = self.by_stem.get_mut(&key) {
                    rows.retain(|(source, _)| *source != id);
                    if rows.is_empty() {
                        self.by_stem.remove(&key);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn is_indexed(&self, id: EntryId) -> bool {
        self.sources.contains_key(&id)
    }

    #[must_use]
    pub fn resolve(&self, source: EntryId, target: &Target) -> Resolution {
        self.notes.resolve(source, target)
    }

    #[must_use]
    pub fn resolve_link(&self, source: EntryId, link: &Link) -> Resolution {
        self.resolve(source, &Target::parse(link))
    }

    /// Grouped by source in vault order; a note linking to itself is included.
    #[must_use]
    pub fn backlinks(&self, note: EntryId) -> Vec<Backlink<'_>> {
        let Some(rows) = self
            .notes
            .stem(note)
            .and_then(|stem| self.by_stem.get(stem))
        else {
            return Vec::new();
        };
        let mut grouped: BTreeMap<(usize, EntryId), Vec<usize>> = BTreeMap::new();
        // A shared name such as `index` is written the same way by thousands of links, and
        // only a relative link's resolution depends on its source.
        let mut by_text: HashMap<(&str, Style), Resolution> = HashMap::new();
        let mut by_query: HashMap<Query, Resolution> = HashMap::new();
        for &(source, at) in rows {
            let link = &self.sources[&source].links[at];
            let resolution = match &link.target {
                Target::Note { path, style, .. } if *style != Style::Relative => by_text
                    .entry((path, *style))
                    .or_insert_with(|| self.resolve(source, &link.target))
                    .clone(),
                target => match self.notes.query(source, target) {
                    Ok(query) => by_query
                        .entry(query)
                        .or_insert_with_key(|query| self.notes.search(query))
                        .clone(),
                    Err(resolution) => resolution,
                },
            };
            if resolution == Resolution::Note(note) {
                grouped
                    .entry((self.notes.order(source), source))
                    .or_default()
                    .push(at);
            }
        }
        grouped
            .into_iter()
            .map(|((_, source), mut at)| {
                at.sort_unstable();
                let links = &self.sources[&source].links;
                Backlink {
                    source,
                    links: at.into_iter().map(|at| &links[at]).collect(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
