use inkra_vault::{Entry, EntryId, Kind, Mtime};

use super::building::Built;

/// What changed in the vault since the entries the index last applied, derived off the foreground.
pub(super) enum Change {
    /// The same notes, at the same paths, in the same order: the notes whose mtime moved.
    Mtimes(Vec<(EntryId, Mtime)>),
    Layout(Box<Built>),
}

impl Change {
    /// `last` is `None` until the first build applies, which is always a `Layout`.
    pub(super) fn between(last: Option<&[Entry]>, entries: &[Entry]) -> Self {
        match last.and_then(|last| moved_mtimes(last, entries)) {
            Some(moved) => Self::Mtimes(moved),
            None => Self::Layout(Box::new(Built::new(entries))),
        }
    }
}

/// `None` when a note was added, removed, re-keyed, moved or reordered; folders are read by no build.
fn moved_mtimes(last: &[Entry], entries: &[Entry]) -> Option<Vec<(EntryId, Mtime)>> {
    let (mut before, mut after) = (notes(last), notes(entries));
    let mut moved = Vec::new();
    loop {
        match (before.next(), after.next()) {
            (None, None) => return Some(moved),
            (Some(old), Some(new)) if old.id == new.id && old.path == new.path => {
                if old.mtime != new.mtime {
                    moved.push((new.id, new.mtime));
                }
            }
            _ => return None,
        }
    }
}

/// The entries a build reads: folders are dropped.
pub(super) fn notes(entries: &[Entry]) -> impl Iterator<Item = &Entry> {
    entries.iter().filter(|entry| entry.kind == Kind::Note)
}
