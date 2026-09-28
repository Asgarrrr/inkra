use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use crate::chunks::split;
use crate::words::NameWords;

/// One note's name, as `find` ranks it.
struct Name {
    id: EntryId,
    /// NFC, `/`-separated, `.md` stripped.
    path: Arc<str>,
    /// `path.chars().count()`: the path-length tie-break of `find`'s rank.
    chars: usize,
}

/// Each note's name in vault order, as `find` reads it. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct NamesSnapshot {
    names: Arc<[Name]>,
    words: Arc<NameWords>,
}

impl NamesSnapshot {
    pub(crate) fn build(notes: Vec<(EntryId, Arc<str>)>) -> Self {
        let words = NameWords::build(notes.iter().map(|(_, path)| &**path));
        Self {
            names: notes
                .into_iter()
                .map(|(id, path)| Name {
                    id,
                    chars: path.chars().count(),
                    path,
                })
                .collect(),
            words: Arc::new(words),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The name words' bytes and arrays; the names are not counted.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.words.heap_bytes()
    }

    /// Ranges to rank in parallel; merging their results equals one range.
    pub(crate) fn chunks(&self, n: usize) -> Vec<Range<usize>> {
        split(self.names.len(), n)
    }

    pub(crate) fn id(&self, order: usize) -> EntryId {
        self.names[order].id
    }

    pub(crate) fn path(&self, order: usize) -> &Arc<str> {
        &self.names[order].path
    }

    pub(crate) fn chars(&self, order: usize) -> usize {
        self.names[order].chars
    }

    pub(crate) fn words(&self) -> &NameWords {
        &self.words
    }
}

#[cfg(test)]
mod tests;
