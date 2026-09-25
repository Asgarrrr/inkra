use inkra_vault::{EntryId, Kind};
use unicode_normalization::UnicodeNormalization;

use crate::key::strip_md;
use crate::names::NamesSnapshot;

/// The vault's notes, prepared off the foreground so applying them is a swap.
pub struct NoteSet {
    names: NamesSnapshot,
}

impl NoteSet {
    /// `entries` in `Vault::entries()` order, paths relative to the root; folders are dropped.
    #[must_use]
    pub fn build(entries: Vec<(EntryId, String, Kind)>) -> Self {
        let notes = entries
            .into_iter()
            .filter(|(_, _, kind)| *kind == Kind::Note)
            .map(|(id, path, _)| {
                let path = strip_md(&path);
                let path = if path.is_ascii() {
                    path.to_owned()
                } else {
                    path.nfc().collect()
                };
                (id, path)
            });
        Self {
            names: NamesSnapshot::build(notes),
        }
    }

    #[must_use]
    pub fn names(&self) -> NamesSnapshot {
        self.names.clone()
    }
}

#[cfg(test)]
mod tests;
