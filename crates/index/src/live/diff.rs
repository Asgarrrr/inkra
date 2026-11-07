use inkra_vault::{Entry, EntryId, Kind, Stamp};

use super::builds::Layout;

/// What changed in the vault since the entries the index last applied, derived off the foreground.
pub(super) enum Change {
    /// The same files, at the same paths, in the same order: the notes whose stamp moved.
    Stamps(Vec<(EntryId, Stamp)>),
    Layout(Box<Layout>),
}

impl Change {
    /// `last` is `None` until the first build applies, which is always a `Layout`.
    pub(super) fn between(last: Option<&[Entry]>, entries: &[Entry]) -> Self {
        match last.and_then(|last| moved_stamps(last, entries)) {
            Some(moved) => Self::Stamps(moved),
            None => Self::Layout(Box::new(Layout::new(entries))),
        }
    }
}

/// `None` when a file was added, removed, re-keyed, moved or reordered, since links resolve to
/// assets too; folders are read by no build, and an asset's stamp loads nothing.
fn moved_stamps(last: &[Entry], entries: &[Entry]) -> Option<Vec<(EntryId, Stamp)>> {
    let (mut before, mut after) = (files(last), files(entries));
    let mut moved = Vec::new();
    loop {
        match (before.next(), after.next()) {
            (None, None) => return Some(moved),
            (Some(old), Some(new)) if old.id == new.id && old.path == new.path => {
                if new.kind == Kind::Note && old.stamp() != new.stamp() {
                    moved.push((new.id, new.stamp()));
                }
            }
            _ => return None,
        }
    }
}

/// The entries a build loads: folders and assets are dropped.
pub(super) fn notes(entries: &[Entry]) -> impl Iterator<Item = &Entry> {
    entries.iter().filter(|entry| entry.kind == Kind::Note)
}

/// The entries links resolve to: folders are dropped.
pub(super) fn files(entries: &[Entry]) -> impl Iterator<Item = &Entry> {
    entries.iter().filter(|entry| entry.kind.is_file())
}
