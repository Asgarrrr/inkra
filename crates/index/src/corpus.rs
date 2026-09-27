use std::mem;
use std::ops::Range;
use std::sync::Arc;

use inkra_vault::{EntryId, Mtime};

use crate::chunks::split;

/// What a slot costs beyond its text.
pub(crate) const SLOT_BYTES: usize = mem::size_of::<Slot>() + mem::size_of::<EntryId>();
/// What a loaded text costs beyond its bytes: its `Arc`'s strong and weak counts.
pub(crate) const TEXT_HEADER_BYTES: usize = 2 * mem::size_of::<usize>();

/// Slots per chunk; a write under a held snapshot copies one chunk.
const CHUNK: usize = 256;

/// `modified` is the last text's mtime, or the scan's if it never had one.
#[derive(Clone)]
struct Slot {
    text: Option<Arc<str>>,
    modified: Mtime,
}

/// `CHUNK` consecutive slots, fewer in the last chunk.
type Chunk = Arc<[Slot]>;

/// Every note's saved text and its mtime, in vault order.
#[derive(Default)]
pub(crate) struct Corpus {
    ids: Arc<[EntryId]>,
    slots: Arc<[Chunk]>,
    text_bytes: usize,
}

impl Corpus {
    /// Copies the chunk list and one chunk only while a snapshot holds them. Ignored past the end.
    pub(crate) fn write(&mut self, at: usize, text: Arc<str>, modified: Mtime) {
        let Some(slot) = slot_mut(&mut self.slots, at) else {
            return;
        };
        self.text_bytes -= cost(slot.text.as_deref());
        self.text_bytes += cost(Some(&text));
        *slot = Slot {
            text: Some(text),
            modified,
        };
    }

    /// Drops the text and keeps its mtime. Ignored past the end.
    pub(crate) fn clear(&mut self, at: usize) {
        // Clearing an empty slot would copy a chunk for nothing while a snapshot is held.
        if slot(&self.slots, at).is_some_and(|slot| slot.text.is_some())
            && let Some(slot) = slot_mut(&mut self.slots, at)
        {
            self.text_bytes -= cost(slot.text.take().as_deref());
        }
    }

    /// Each note keeps its text and mtime from `order(id)`, else takes its `mtimes` entry.
    /// Returns the previous build's ids and the texts of the notes gone.
    pub(crate) fn reorder(
        &mut self,
        ids: Arc<[EntryId]>,
        mtimes: &[Mtime],
        order: impl Fn(EntryId) -> usize,
    ) -> (Arc<[EntryId]>, Vec<Arc<str>>) {
        assert_eq!(ids.len(), mtimes.len(), "one mtime per note");
        let mut previous: Vec<&mut [Slot]> = Arc::make_mut(&mut self.slots)
            .iter_mut()
            .map(Arc::make_mut)
            .collect();
        let mut text_bytes = 0;
        let slots = ids
            .chunks(CHUNK)
            .zip(mtimes.chunks(CHUNK))
            .map(|(ids, mtimes)| {
                ids.iter()
                    .zip(mtimes)
                    .map(|(&id, &scanned)| {
                        let at = order(id);
                        let kept = previous
                            .get_mut(at / CHUNK)
                            .and_then(|chunk| chunk.get_mut(at % CHUNK))
                            .and_then(|slot| Some((slot.text.take()?, slot.modified)));
                        let (text, modified) = kept.unzip();
                        text_bytes += cost(text.as_deref());
                        Slot {
                            text,
                            modified: modified.unwrap_or(scanned),
                        }
                    })
                    .collect()
            })
            .collect();
        let retired = previous
            .into_iter()
            .flat_map(|chunk| chunk.iter_mut().filter_map(|slot| slot.text.take()))
            .collect();
        self.slots = slots;
        self.text_bytes = text_bytes;
        (mem::replace(&mut self.ids, ids), retired)
    }

    pub(crate) fn snapshot(&self) -> CorpusSnapshot {
        CorpusSnapshot {
            ids: Arc::clone(&self.ids),
            slots: Arc::clone(&self.slots),
            heap_bytes: self.ids.len() * SLOT_BYTES + self.text_bytes,
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

fn cost(text: Option<&str>) -> usize {
    text.map_or(0, |text| TEXT_HEADER_BYTES + text.len())
}

/// The saved text of every note at one moment, in vault order. Cloning is an `Arc` bump.
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

    /// Ranges to search in parallel; `Searched::merge` over their results equals one range.
    #[must_use]
    pub fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        split(self.ids.len(), n)
    }

    /// Each loaded text's bytes and `Arc` counts, plus a fixed cost per note.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.heap_bytes
    }

    pub(crate) fn notes(&self, range: Range<usize>) -> impl Iterator<Item = (EntryId, &str)> {
        let slots = self.slots[range.start / CHUNK..]
            .iter()
            .flat_map(|chunk| chunk.iter())
            .skip(range.start % CHUNK);
        self.ids[range]
            .iter()
            .zip(slots)
            .filter_map(|(id, slot)| Some((*id, slot.text.as_deref()?)))
    }

    /// The note at `order` and its text; `None` past the end or while it is not loaded.
    pub(crate) fn text(&self, order: usize) -> Option<(EntryId, &str)> {
        Some((
            *self.ids.get(order)?,
            slot(&self.slots, order)?.text.as_deref()?,
        ))
    }

    /// The mtime of the last text at `order`, or the scan's if it never had one; `None` past the end.
    pub(crate) fn modified(&self, order: usize) -> Option<Mtime> {
        Some(slot(&self.slots, order)?.modified)
    }
}

#[cfg(test)]
mod tests;
