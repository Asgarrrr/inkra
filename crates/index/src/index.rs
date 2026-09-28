use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use inkra_markdown::Link;
use inkra_vault::{EntryId, Mtime};

use crate::content::NoteContent;
use crate::corpus::{Corpus, CorpusSnapshot};
use crate::key::Key;
use crate::names::NamesSnapshot;
use crate::note_set::NoteSet;
use crate::notes::{NoteTable, Query, Resolution};
use crate::source::{Source, SourceLink};
use crate::target::{Style, Target};
use crate::words::{
    Lookup, MergeWhen, MergedWords, NoteQuery, RetiredWords, Words, WordsMerge, WordsSnapshot,
};

/// Every note's links, resolved on demand against the current notes, so a create, rename or
/// trash invalidates nothing.
#[derive(Default)]
pub struct NoteIndex {
    notes: NoteTable,
    names: NamesSnapshot,
    /// The build's scanned mtimes, handed back whole in the set `set_notes` returns.
    mtimes: Arc<[Mtime]>,
    sources: HashMap<EntryId, Source>,
    /// A note link's stem key → (source, index into its links). Every link resolving to `T`
    /// has `T`'s stem, so this finds backlinks without a reverse map.
    by_stem: HashMap<Key, Vec<(EntryId, usize)>>,
    corpus: Corpus,
    words: Words,
}

/// The links of one source that resolve to a note, in document order.
#[derive(Debug, PartialEq, Eq)]
pub struct Backlink<'a> {
    pub source: EntryId,
    pub links: Vec<&'a SourceLink>,
}

impl NoteIndex {
    /// Replaces the notes links resolve against and quick open lists, reorders the corpus, and
    /// drops the source, text and words of every note gone. The caller frees the previous set,
    /// which holds those texts, so a 50k table need not be freed on the foreground.
    #[must_use]
    pub fn set_notes(&mut self, notes: NoteSet) -> NoteSet {
        let (ids, retired) = self
            .corpus
            .reorder(Arc::clone(&notes.ids), &notes.mtimes, |id| {
                self.notes.order(id)
            });
        let previous = NoteSet {
            notes: mem::replace(&mut self.notes, notes.notes),
            names: mem::replace(&mut self.names, notes.names),
            ids,
            mtimes: mem::replace(&mut self.mtimes, notes.mtimes),
            _retired: retired,
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
        self.words.remove_all(&gone);
        previous
    }

    /// Each note's name of the last `set_notes`, as `find` reads it. Cloning is an `Arc` bump.
    #[must_use]
    pub fn names(&self) -> NamesSnapshot {
        self.names.clone()
    }

    /// The saved text of every note. Cloning is an `Arc` bump; a later write never reaches it.
    #[must_use]
    pub fn corpus(&self) -> CorpusSnapshot {
        self.corpus.snapshot()
    }

    /// The words of every loaded note. Cloning is an `Arc` bump; a later write never reaches it.
    #[must_use]
    pub fn words(&self) -> WordsSnapshot {
        self.words.snapshot()
    }

    /// The names, corpus and words at this moment, so names and corpus are of one build.
    #[must_use]
    pub fn snapshot(&self) -> IndexSnapshot {
        IndexSnapshot {
            names: self.names(),
            corpus: self.corpus(),
            words: self.words(),
        }
    }

    /// A word merge when `when` asks for one and none runs; run it off the foreground.
    #[must_use]
    pub fn words_merge(&mut self, when: MergeWhen) -> Option<WordsMerge> {
        self.words.merge(when)
    }

    /// No note is fresh and no merge runs: lookups read the merged segment alone.
    #[must_use]
    pub fn words_settled(&self) -> bool {
        !self.words.has_fresh() && !self.words.is_merging()
    }

    /// Swaps a merge's result in unless it is another index's; free the return off the foreground.
    #[must_use = "free the retired words off the foreground"]
    pub fn words_apply(&mut self, merged: MergedWords) -> RetiredWords {
        self.words.apply(merged)
    }

    /// Starts, runs and applies merges until none is left to do; a running merge blocks it.
    #[cfg(test)]
    pub(crate) fn merge_all(&mut self) {
        self.words.merge_all();
    }

    /// Replaces `id`'s links, text and words. Ignored for an id that is not a note, so a load
    /// finishing after its note was trashed leaves nothing behind.
    pub fn insert(&mut self, id: EntryId, content: NoteContent) {
        if !self.notes.contains(id) {
            return;
        }
        self.remove_links(id);
        let NoteContent {
            source,
            words,
            text,
            modified,
        } = content;
        for (at, link) in source.links.iter().enumerate() {
            if let Target::Note { path, .. } = &link.target {
                self.by_stem
                    .entry(Key::stem(path))
                    .or_default()
                    .push((id, at));
            }
        }
        self.sources.insert(id, source);
        self.corpus.write(self.notes.order(id), text, modified);
        self.words.set(id, words);
    }

    pub fn remove(&mut self, id: EntryId) {
        self.words.remove(id);
        self.corpus.clear(self.notes.order(id));
        self.remove_links(id);
    }

    /// `insert` calls this, not `remove`, so a reload keeps the note's word ordinal.
    fn remove_links(&mut self, id: EntryId) {
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

/// Taken at one moment for `find_notes`, `merge_notes` and `excerpt`; a clone bumps three `Arc`s.
#[derive(Clone)]
pub struct IndexSnapshot {
    names: NamesSnapshot,
    corpus: CorpusSnapshot,
    words: WordsSnapshot,
}

impl IndexSnapshot {
    /// Each term's notes, as `WordsSnapshot::lookup` gives them; `None` if cancelled.
    #[must_use]
    pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Option<Lookup> {
        self.words.lookup(query, cancel)
    }

    /// Ranges to search in parallel; `merge_notes` over their results equals one range.
    #[must_use]
    pub fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        self.names.chunks(n)
    }

    pub(crate) fn names(&self) -> &NamesSnapshot {
        &self.names
    }

    pub(crate) fn corpus(&self) -> &CorpusSnapshot {
        &self.corpus
    }
}

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod words_tests;
