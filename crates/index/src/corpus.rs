use std::mem;
use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use crate::chunks::split;

/// What a slot costs beyond its text.
pub(crate) const SLOT_BYTES: usize = mem::size_of::<Option<Arc<str>>>() + mem::size_of::<EntryId>();
/// What a loaded text costs beyond its bytes: its `Arc`'s strong and weak counts.
pub(crate) const TEXT_HEADER_BYTES: usize = 2 * mem::size_of::<usize>();

/// Slots per chunk; a write under a held snapshot copies one chunk.
const CHUNK: usize = 256;

/// `CHUNK` consecutive slots, fewer in the last chunk.
type Chunk = Arc<[Option<Arc<str>>]>;

/// Every note's saved text in vault order; `None` is a note not loaded yet.
#[derive(Default)]
pub(crate) struct Corpus {
    ids: Arc<[EntryId]>,
    texts: Arc<[Chunk]>,
    text_bytes: usize,
}

impl Corpus {
    /// Copies the chunk list and one chunk only while a snapshot holds them. Ignored past the end.
    pub(crate) fn set(&mut self, at: usize, text: Option<Arc<str>>) {
        let (chunk, at) = (at / CHUNK, at % CHUNK);
        // `None` over `None` would copy a chunk for nothing while a snapshot is held.
        if self
            .texts
            .get(chunk)
            .and_then(|chunk| chunk.get(at))
            .is_none_or(|slot| slot.is_none() && text.is_none())
        {
            return;
        }
        let slot = &mut Arc::make_mut(&mut Arc::make_mut(&mut self.texts)[chunk])[at];
        self.text_bytes -= cost(slot.as_deref());
        self.text_bytes += cost(text.as_deref());
        *slot = text;
    }

    /// The notes of a new build, each keeping the text it had at `order(id)`; one pass.
    /// Returns the previous ids and the texts of the notes gone.
    pub(crate) fn reorder(
        &mut self,
        ids: Arc<[EntryId]>,
        order: impl Fn(EntryId) -> usize,
    ) -> (Arc<[EntryId]>, Vec<Arc<str>>) {
        let mut previous: Vec<&mut [Option<Arc<str>>]> = Arc::make_mut(&mut self.texts)
            .iter_mut()
            .map(Arc::make_mut)
            .collect();
        let mut text_bytes = 0;
        let texts = ids
            .chunks(CHUNK)
            .map(|ids| {
                ids.iter()
                    .map(|&id| {
                        let at = order(id);
                        let text = previous
                            .get_mut(at / CHUNK)
                            .and_then(|chunk| chunk.get_mut(at % CHUNK))
                            .and_then(Option::take);
                        text_bytes += cost(text.as_deref());
                        text
                    })
                    .collect()
            })
            .collect();
        let retired = previous
            .into_iter()
            .flat_map(|chunk| chunk.iter_mut().filter_map(Option::take))
            .collect();
        self.texts = texts;
        self.text_bytes = text_bytes;
        (mem::replace(&mut self.ids, ids), retired)
    }

    pub(crate) fn snapshot(&self) -> CorpusSnapshot {
        CorpusSnapshot {
            ids: Arc::clone(&self.ids),
            texts: Arc::clone(&self.texts),
            heap_bytes: self.ids.len() * SLOT_BYTES + self.text_bytes,
        }
    }
}

fn cost(text: Option<&str>) -> usize {
    text.map_or(0, |text| TEXT_HEADER_BYTES + text.len())
}

/// The saved text of every note at one moment, in vault order. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct CorpusSnapshot {
    ids: Arc<[EntryId]>,
    texts: Arc<[Chunk]>,
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
        let texts = self.texts[range.start / CHUNK..]
            .iter()
            .flat_map(|chunk| chunk.iter())
            .skip(range.start % CHUNK);
        self.ids[range]
            .iter()
            .zip(texts)
            .filter_map(|(id, text)| Some((*id, text.as_deref()?)))
    }
}

#[cfg(test)]
mod tests;
