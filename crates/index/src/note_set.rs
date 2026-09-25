use std::sync::Arc;

use inkra_vault::{EntryId, Kind};
use unicode_normalization::UnicodeNormalization;

use crate::key::strip_md;
use crate::names::NamesSnapshot;
use crate::notes::NoteTable;

/// The vault's notes, prepared off the foreground so applying them is a swap.
pub struct NoteSet {
    pub(crate) notes: NoteTable,
    pub(crate) names: NamesSnapshot,
}

impl NoteSet {
    /// `entries` in `Vault::entries()` order, paths relative to the root; folders are dropped.
    #[must_use]
    pub fn build(entries: Vec<(EntryId, String, Kind)>) -> Self {
        let notes = prepare(entries);
        Self {
            notes: NoteTable::new(&notes),
            names: NamesSnapshot::build(notes),
        }
    }

    #[must_use]
    pub fn names(&self) -> NamesSnapshot {
        self.names.clone()
    }
}

/// The notes of `entries` in order, `.md` stripped, NFC: the one pass both halves share.
pub(crate) fn prepare(entries: Vec<(EntryId, String, Kind)>) -> Vec<(EntryId, Arc<str>)> {
    entries
        .into_iter()
        .filter(|(_, _, kind)| *kind == Kind::Note)
        .map(|(id, path, _)| {
            let path = strip_md(&path);
            let path: Arc<str> = if path.is_ascii() {
                path.into()
            } else {
                path.nfc().collect::<String>().into()
            };
            (id, path)
        })
        .collect()
}

#[cfg(test)]
mod tests;
