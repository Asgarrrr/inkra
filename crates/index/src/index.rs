use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use inkra_markdown::Link;
use inkra_vault::{EntryId, Mtime, Stamp};

use crate::content::{FileRead, NoteContent};
use crate::corpus::{Corpus, CorpusSnapshot};
use crate::key::Key;
use crate::names::NamesSnapshot;
use crate::note_set::NoteSet;
use crate::notes::{NoteTable, Query, Resolution};
use crate::seed::Seed;
use crate::source::{Source, SourceLink};
use crate::target::{Style, Target};
use crate::words::{
    Lookup, MergeWhen, MergedWords, NoteQuery, RetiredWords, Shards, Words, WordsMerge,
    WordsSnapshot,
};

/// Every note's links, resolved on demand against the current notes, so a create, rename or
/// trash invalidates nothing.
#[derive(Default)]
pub struct NoteIndex {
    notes: NoteTable,
    names: NamesSnapshot,
    /// The build's scanned mtimes, handed back whole in the set `set_notes` returns.
    mtimes: Arc<[Mtime]>,
    sources: Shards<EntryId, Arc<Source>>,
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
    /// drops the source, stamp and words of every note gone. The caller frees the previous set,
    /// so a 50k table need not be freed on the foreground.
    #[must_use]
    pub fn set_notes(&mut self, notes: NoteSet) -> NoteSet {
        let ids = self
            .corpus
            .reorder(Arc::clone(&notes.ids), &notes.mtimes, |id| {
                self.notes.order(id)
            });
        let previous = NoteSet {
            notes: mem::replace(&mut self.notes, notes.notes),
            names: mem::replace(&mut self.names, notes.names),
            ids,
            mtimes: mem::replace(&mut self.mtimes, notes.mtimes),
        };
        let gone: HashSet<EntryId> = self
            .sources
            .iter()
            .map(|(&id, _)| id)
            .filter(|&id| !self.notes.contains(id))
            .collect();
        let mut stems = HashSet::new();
        for source in gone.iter().filter_map(|&id| self.sources.remove(id)) {
            stems.extend(source.note_stems().map(|(_, key)| key));
        }
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

    /// Every note's mtime and loaded stamp. Cloning is an `Arc` bump; a later write never reaches it.
    #[must_use]
    pub fn corpus(&self) -> CorpusSnapshot {
        self.corpus.snapshot()
    }

    /// The words of every loaded note. Cloning is an `Arc` bump; a later write never reaches it.
    #[must_use]
    pub fn words(&self) -> WordsSnapshot {
        self.words.snapshot()
    }

    /// The names, corpus, words and sources at this moment, so names and corpus are of one build.
    #[must_use]
    pub fn snapshot(&self) -> IndexSnapshot {
        IndexSnapshot {
            names: self.names(),
            corpus: self.corpus(),
            words: self.words(),
            sources: self.sources.clone(),
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

    /// `self` with `words` in place of its own.
    #[cfg(test)]
    pub(crate) fn with_words(mut self, words: Words) -> Self {
        self.words = words;
        self
    }

    /// Moves a seed's words, corpus, sources and stems in, then puts back each note loaded or
    /// saved before it, so its own read and links win (P6). Returns the stamps to pre-fill
    /// `indexed` with.
    ///
    /// # Panics
    ///
    /// If a merge applied before the seed, or a held note has no read.
    pub(crate) fn seed(&mut self, seed: Seed) -> Vec<(EntryId, Stamp)> {
        let Seed {
            words,
            corpus,
            sources,
            by_stem,
            stamps,
            pruned: _,
        } = seed;
        let mut held_words = mem::replace(&mut self.words, words).into_fresh();
        let held_reads = mem::replace(&mut self.corpus, corpus).snapshot();
        let held = mem::replace(&mut self.sources, sources);
        self.by_stem = by_stem;
        for (&id, source) in held.iter() {
            let Some(read) = held_reads.read(self.notes.order(id)) else {
                panic!("a held note has no read");
            };
            let words = held_words.remove(&id).unwrap_or_default();
            self.insert(
                id,
                NoteContent {
                    source: (**source).clone(),
                    words,
                    read,
                },
            );
        }
        stamps
    }

    /// Replaces `id`'s links, stamp and words. Ignored for an id that is not a note, so a load
    /// finishing after its note was trashed leaves nothing behind.
    pub fn insert(&mut self, id: EntryId, content: NoteContent) {
        if !self.notes.contains(id) {
            return;
        }
        self.remove_links(id);
        let NoteContent {
            source,
            words,
            read,
        } = content;
        for (at, key) in source.note_stems() {
            self.by_stem.entry(key).or_default().push((id, at));
        }
        self.sources.insert(id, Arc::new(source));
        self.corpus.write(self.notes.order(id), read);
        self.words.set(id, words);
    }

    pub fn remove(&mut self, id: EntryId) {
        self.words.remove(id);
        self.corpus.clear(self.notes.order(id));
        self.remove_links(id);
    }

    /// `insert` calls this, not `remove`, so a reload keeps the note's word ordinal.
    fn remove_links(&mut self, id: EntryId) {
        let Some(old) = self.sources.remove(id) else {
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
        self.sources.contains_key(id)
    }

    #[must_use]
    pub fn resolve(&self, source: Option<EntryId>, target: &Target) -> Resolution {
        self.notes.resolve(source, target)
    }

    #[must_use]
    pub fn resolve_link(&self, source: Option<EntryId>, link: &Link) -> Resolution {
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
            let link = &self.source(source).links[at];
            let resolution = match &link.target {
                Target::Note { path, style, .. } if *style != Style::Relative => by_text
                    .entry((path, *style))
                    .or_insert_with(|| self.resolve(Some(source), &link.target))
                    .clone(),
                target => match self.notes.query(Some(source), target) {
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
                let links = &self.source(source).links;
                Backlink {
                    source,
                    links: at.into_iter().map(|at| &links[at]).collect(),
                }
            })
            .collect()
    }

    fn source(&self, id: EntryId) -> &Source {
        let Some(source) = self.sources.get(id) else {
            panic!("note {id:?} has a backlink row but holds no source");
        };
        source
    }
}

/// Taken at one moment for `find_notes`, `search`, `excerpt` and the cache file; a clone bumps
/// four `Arc`s.
#[derive(Clone)]
pub struct IndexSnapshot {
    names: NamesSnapshot,
    corpus: CorpusSnapshot,
    words: WordsSnapshot,
    sources: Shards<EntryId, Arc<Source>>,
}

impl IndexSnapshot {
    /// Each term's notes, as `WordsSnapshot::lookup` gives them; `None` if cancelled.
    #[must_use]
    pub(crate) fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Option<Lookup> {
        self.words.lookup(query, cancel)
    }

    /// Ranges to search in parallel; `merge_found` over their results equals one range.
    #[must_use]
    pub(crate) fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        self.names.chunks(n)
    }

    pub(crate) fn names(&self) -> &NamesSnapshot {
        &self.names
    }

    pub(crate) fn corpus(&self) -> &CorpusSnapshot {
        &self.corpus
    }

    pub(crate) fn words(&self) -> &WordsSnapshot {
        &self.words
    }

    /// Each loaded note in vault order: its id, its file as scanned, the read its words came
    /// from, and its links.
    ///
    /// # Panics
    /// If a note was loaded but holds no source.
    pub(crate) fn loaded(&self) -> impl Iterator<Item = (EntryId, &Arc<str>, FileRead, &Source)> {
        (0..self.names.len()).filter_map(move |order| {
            let read = self.corpus.read(order)?;
            let id = self.names.id(order);
            let Some(source) = self.sources.get(id) else {
                panic!("note {id:?} was loaded but holds no source");
            };
            Some((id, self.names.file(order), read, &**source))
        })
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
