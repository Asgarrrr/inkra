use std::mem;
use std::sync::Arc;

use inkra_vault::{EntryId, Mtime, Stamp};

/// What one note costs: its slot and its id.
pub(crate) const SLOT_BYTES: usize = mem::size_of::<Slot>() + mem::size_of::<EntryId>();

/// Slots per chunk; a write under a held snapshot copies one chunk.
const CHUNK: usize = 256;

/// `modified` is the last text's mtime, or the scan's if it never had one. `stamp` is `Some`
/// while the note is loaded: the stamp of the text its words came from.
#[derive(Clone)]
struct Slot {
    modified: Mtime,
    stamp: Option<Stamp>,
}

/// `CHUNK` consecutive slots, fewer in the last chunk.
type Chunk = Arc<[Slot]>;

/// Every note's mtime and loaded stamp, in vault order.
#[derive(Default)]
pub(crate) struct Corpus {
    ids: Arc<[EntryId]>,
    slots: Arc<[Chunk]>,
}

impl Corpus {
    /// Copies the chunk list and one chunk only while a snapshot holds them. Ignored past the end.
    pub(crate) fn write(&mut self, at: usize, stamp: Stamp) {
        if let Some(slot) = slot_mut(&mut self.slots, at) {
            *slot = Slot {
                modified: stamp.mtime,
                stamp: Some(stamp),
            };
        }
    }

    /// Drops the stamp, and keeps the mtime. Ignored past the end.
    pub(crate) fn clear(&mut self, at: usize) {
        // Clearing an empty slot would copy a chunk for nothing while a snapshot is held.
        if slot(&self.slots, at).is_some_and(|slot| slot.stamp.is_some())
            && let Some(slot) = slot_mut(&mut self.slots, at)
        {
            slot.stamp = None;
        }
    }

    /// Each note loaded at `order(id)` keeps its mtime and stamp there, else takes its `mtimes`
    /// entry. Returns the previous build's ids.
    pub(crate) fn reorder(
        &mut self,
        ids: Arc<[EntryId]>,
        mtimes: &[Mtime],
        order: impl Fn(EntryId) -> usize,
    ) -> Arc<[EntryId]> {
        assert_eq!(ids.len(), mtimes.len(), "one mtime per note");
        let previous = &self.slots;
        let slots = ids
            .chunks(CHUNK)
            .zip(mtimes.chunks(CHUNK))
            .map(|(ids, mtimes)| {
                ids.iter()
                    .zip(mtimes)
                    .map(|(&id, &scanned)| match slot(previous, order(id)) {
                        Some(kept) if kept.stamp.is_some() => kept.clone(),
                        _ => Slot {
                            modified: scanned,
                            stamp: None,
                        },
                    })
                    .collect()
            })
            .collect();
        self.slots = slots;
        mem::replace(&mut self.ids, ids)
    }

    pub(crate) fn snapshot(&self) -> CorpusSnapshot {
        CorpusSnapshot {
            ids: Arc::clone(&self.ids),
            slots: Arc::clone(&self.slots),
            heap_bytes: self.ids.len() * SLOT_BYTES,
        }
    }
}

fn slot(slots: &[Chunk], at: usize) -> Option<&Slot> {
    slots.get(at / CHUNK)?.get(at % CHUNK)
}

/// Checks the bounds before `make_mut`, so a write past the end copies nothing.
fn slot_mut(slots: &mut Arc<[Chunk]>, at: usize) -> Option<&mut Slot> {
    slot(slots, at)?;
    Some(&mut Arc::make_mut(&mut Arc::make_mut(slots)[at / CHUNK])[at % CHUNK])
}

/// Every note's mtime and loaded stamp at one moment, in vault order. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct CorpusSnapshot {
    ids: Arc<[EntryId]>,
    slots: Arc<[Chunk]>,
    heap_bytes: usize,
}

impl CorpusSnapshot {
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// A fixed cost per note.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.heap_bytes
    }

    /// The mtime of the last text at `order`, or the scan's if it never had one; `None` past the end.
    pub(crate) fn modified(&self, order: usize) -> Option<Mtime> {
        Some(slot(&self.slots, order)?.modified)
    }

    /// The stamp of the text the words at `order` came from; `None` past the end or while it is
    /// not loaded.
    pub(crate) fn stamp(&self, order: usize) -> Option<Stamp> {
        slot(&self.slots, order)?.stamp
    }
}

#[cfg(test)]
mod tests;
