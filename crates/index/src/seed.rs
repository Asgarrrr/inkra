//! The cache over a fresh scan: each cached note matched to an entry, rekeyed off the
//! foreground, so the foreground only moves the maps (persisted index spec, P5, P6, P8).

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use inkra_vault::{Entry, EntryId, Stamp};

use crate::content::FileRead;
use crate::corpus::Corpus;
use crate::note_set::NoteSet;
use crate::persist::{CachedNote, CachedWords, Decoded, SCRUB, scrub_sample};
use crate::source::Source;
use crate::stems::Stems;
use crate::words::{Alive, Ordinal, Ordinals, Shards, Words};

/// What `NoteIndex::seed` moves in, keyed by the scan's ids.
pub(crate) struct Seed {
    pub(crate) words: Words,
    pub(crate) corpus: Corpus,
    pub(crate) sources: Shards<EntryId, Arc<Source>>,
    pub(crate) by_stem: Stems,
    /// Each seeded note's cached stamp, the racy ones left out so they reload (P8).
    pub(crate) stamps: Vec<(EntryId, Stamp)>,
}

/// What the seed found beyond the maps; `LiveIndex` keeps it until `indexed`.
pub(crate) struct LaunchFacts {
    /// Some cached note matched no entry or had no stamp: the file holds a note the index does not.
    pub(crate) pruned: bool,
    /// The notes of `stamps` the scrub re-parses, drawn from the file's crc (P16).
    pub(crate) scrub: Vec<EntryId>,
}

impl Seed {
    /// `entries` are the build's notes, and `notes` the set built from them.
    ///
    /// # Panics
    ///
    /// If the decoded words break an invariant the store keeps.
    pub(crate) fn build<'a>(
        decoded: Decoded,
        entries: impl Iterator<Item = &'a Entry>,
        notes: &NoteSet,
    ) -> (Self, LaunchFacts) {
        let Decoded {
            notes: cached,
            words,
            crc,
        } = decoded;
        let entries: Vec<&Entry> = entries.collect();
        let matched = matches(&cached, &entries);
        let mut corpus = Corpus::new(Arc::clone(&notes.ids), &notes.mtimes);
        let mut named = Vec::with_capacity(cached.len());
        let mut sources = Shards::default();
        let mut by_stem = Stems::default();
        let mut stamps = Vec::with_capacity(cached.len());
        let mut pruned = false;
        for (note, at) in cached.into_iter().zip(matched) {
            let (Some(at), Some(stamp)) = (at, note.stamp) else {
                pruned = true;
                continue;
            };
            let id = entries[at].id;
            if let Some(ordinal) = note.ordinal {
                named.push((id, ordinal));
            }
            let read = FileRead {
                stamp,
                identity: note.identity,
            };
            corpus.write(notes.notes.order(id), read);
            by_stem.insert(id, &note.source);
            sources.insert(id, Arc::new(note.source));
            if !is_racy(stamp) {
                stamps.push((id, stamp));
            }
        }
        let words = restore_words(words, named);
        let scrub = scrub_sample(crc, stamps.len(), SCRUB)
            .into_iter()
            .map(|at| stamps[at].0)
            .collect();
        (
            Self {
                words,
                corpus,
                sources,
                by_stem,
                stamps,
            },
            LaunchFacts { pruned, scrub },
        )
    }
}

/// `words` with only the `named` ordinals alive; the ghosts' lengths leave the total.
///
/// # Panics
///
/// If the decoded words break an invariant the store keeps.
pub(crate) fn restore_words(words: CachedWords, named: Vec<(EntryId, Ordinal)>) -> Words {
    let kept: Alive = named.iter().map(|&(_, ordinal)| ordinal).collect();
    let (alive, ghosts): (Vec<_>, Vec<_>) = words
        .alive
        .iter()
        .partition(|&ordinal| kept.contains(ordinal));
    let ghost_words: u64 = ghosts
        .into_iter()
        .map(|ordinal| u64::from(words.segment.length(ordinal)))
        .sum();
    Words::restore(
        words.segment,
        alive.into_iter().collect::<Alive>(),
        named.into_iter().collect::<Ordinals>(),
        words.next,
        words.total - ghost_words,
    )
}

/// Per cached note, the entry it is: by path, else by identity one to one among the notes and
/// entries no path matched (P5). A note with no stamp reloads, so it claims no entry.
fn matches(cached: &[CachedNote], entries: &[&Entry]) -> Vec<Option<usize>> {
    // Lossy, as the cache wrote the path: two names that fold to one match nothing by path.
    let mut by_path: HashMap<Cow<'_, str>, Option<usize>> = HashMap::new();
    for (at, entry) in entries.iter().enumerate() {
        unique(&mut by_path, entry.path.as_path().to_string_lossy(), at);
    }
    let mut matched: Vec<Option<usize>> = cached
        .iter()
        .map(|note| {
            by_path
                .get(&*note.path)
                .copied()
                .flatten()
                .filter(|_| note.stamp.is_some())
        })
        .collect();
    let mut taken = vec![false; entries.len()];
    for &at in matched.iter().flatten() {
        taken[at] = true;
    }
    let mut notes = HashMap::new();
    for (n, note) in cached.iter().enumerate() {
        if matched[n].is_none() && note.stamp.is_some() {
            unique(&mut notes, note.identity, n);
        }
    }
    let mut scanned = HashMap::new();
    for (at, entry) in entries.iter().enumerate() {
        if !taken[at] {
            unique(&mut scanned, entry.identity, at);
        }
    }
    for (identity, n) in notes {
        if let (Some(n), Some(&Some(at))) = (n, scanned.get(&identity)) {
            matched[n] = Some(at);
        }
    }
    matched
}

/// Holds `at` under `key`, or `None` once a second claims it: an ambiguous key matches nothing.
fn unique<K: Hash + Eq>(map: &mut HashMap<K, Option<usize>>, key: K, at: usize) {
    map.entry(key)
        .and_modify(|held| *held = None)
        .or_insert(Some(at));
}

/// A whole-second mtime may hide a same-size write in the tick after the load (P8).
fn is_racy(stamp: Stamp) -> bool {
    stamp
        .mtime
        .0
        .duration_since(UNIX_EPOCH)
        .ok()
        .is_none_or(|since| since.subsec_nanos() == 0)
}
