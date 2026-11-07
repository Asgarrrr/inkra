use std::sync::Arc;

use inkra_vault::{EntryId, Kind, Mtime};
use unicode_normalization::UnicodeNormalization;

use crate::key::strip_md;
use crate::names::NamesSnapshot;
use crate::notes::NoteTable;

/// The vault's notes, prepared off the foreground so applying them is a swap.
pub struct NoteSet {
    pub(crate) notes: NoteTable,
    /// Keyed by full name and path, extension kept; searched only after `notes` misses (K6).
    pub(crate) assets: NoteTable,
    pub(crate) names: NamesSnapshot,
    /// Vault order, as the corpus stores its slots.
    pub(crate) ids: Arc<[EntryId]>,
    /// Each note's scanned mtime, in vault order.
    pub(crate) mtimes: Arc<[Mtime]>,
}

impl NoteSet {
    /// `entries` in `Vault::entries()` order, paths relative to the root; folders are dropped.
    #[must_use]
    pub fn build<P: AsRef<str> + Into<Arc<str>>>(entries: Vec<(EntryId, P, Kind, Mtime)>) -> Self {
        let Prepared {
            notes,
            files,
            mtimes,
            assets,
        } = prepare(entries);
        let table = NoteTable::new(&notes);
        let ids = notes.iter().map(|(id, _)| *id).collect();
        Self {
            notes: table,
            assets: NoteTable::new(&assets),
            names: NamesSnapshot::build(notes, files),
            ids,
            mtimes: mtimes.into(),
        }
    }
}

/// The files of a vault, each column in vault order.
pub(crate) struct Prepared {
    /// `.md` stripped, NFC.
    pub(crate) notes: Vec<(EntryId, Arc<str>)>,
    /// Each note relative to the root as scanned.
    pub(crate) files: Vec<Arc<str>>,
    pub(crate) mtimes: Vec<Mtime>,
    /// The full path, NFC.
    pub(crate) assets: Vec<(EntryId, Arc<str>)>,
}

/// One pass both halves share.
pub(crate) fn prepare<P: AsRef<str> + Into<Arc<str>>>(
    entries: Vec<(EntryId, P, Kind, Mtime)>,
) -> Prepared {
    let mut prepared = Prepared {
        notes: Vec::new(),
        files: Vec::new(),
        mtimes: Vec::new(),
        assets: Vec::new(),
    };
    for (id, file, kind, mtime) in entries {
        match kind {
            Kind::Note => {
                prepared.notes.push((id, nfc(strip_md(file.as_ref()))));
                prepared.files.push(file.into());
                prepared.mtimes.push(mtime);
            }
            Kind::Asset => prepared.assets.push((id, nfc(file))),
            Kind::Dir => {}
        }
    }
    prepared
}

fn nfc<P: AsRef<str> + Into<Arc<str>>>(path: P) -> Arc<str> {
    if path.as_ref().is_ascii() {
        path.into()
    } else {
        path.as_ref().nfc().collect::<String>().into()
    }
}

#[cfg(test)]
mod tests;
