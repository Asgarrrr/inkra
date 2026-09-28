use std::sync::Arc;

use inkra_vault::{EntryId, Kind, Mtime};
use unicode_normalization::UnicodeNormalization;

use crate::key::strip_md;
use crate::names::NamesSnapshot;
use crate::notes::NoteTable;

/// The vault's notes, prepared off the foreground so applying them is a swap.
pub struct NoteSet {
    pub(crate) notes: NoteTable,
    pub(crate) names: NamesSnapshot,
    /// Vault order, as the corpus stores its slots.
    pub(crate) ids: Arc<[EntryId]>,
    /// Each note's scanned mtime, in vault order.
    pub(crate) mtimes: Arc<[Mtime]>,
    /// The texts of the notes this set's successor dropped, freed with it.
    pub(crate) _retired: Vec<Arc<str>>,
}

impl NoteSet {
    /// `entries` in `Vault::entries()` order, paths relative to the root; folders are dropped.
    #[must_use]
    pub fn build(entries: Vec<(EntryId, String, Kind, Mtime)>) -> Self {
        let (notes, mtimes) = prepare(entries);
        let table = NoteTable::new(&notes);
        let ids = notes.iter().map(|(id, _)| *id).collect();
        Self {
            notes: table,
            names: NamesSnapshot::build(notes),
            ids,
            mtimes: mtimes.into(),
            _retired: Vec::new(),
        }
    }
}

/// One pass both halves share: the notes in order, `.md` stripped, NFC, with their mtimes apart.
pub(crate) fn prepare(
    entries: Vec<(EntryId, String, Kind, Mtime)>,
) -> (Vec<(EntryId, Arc<str>)>, Vec<Mtime>) {
    entries
        .into_iter()
        .filter(|(_, _, kind, _)| *kind == Kind::Note)
        .map(|(id, path, _, mtime)| {
            let path = strip_md(&path);
            let path: Arc<str> = if path.is_ascii() {
                path.into()
            } else {
                path.nfc().collect::<String>().into()
            };
            ((id, path), mtime)
        })
        .unzip()
}

#[cfg(test)]
mod tests;
