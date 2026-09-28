//! An open note gone from disk takes back a file at its last path (watcher spec, F9).

use std::sync::Arc;

use super::{EntryId, Key, Vault, compare};
use crate::RelPath;
use crate::fs::Kind;

impl Vault {
    /// The note at `path`; `None` for a folder, whatever its name.
    #[must_use]
    pub(crate) fn id_at(&self, path: &RelPath) -> Option<EntryId> {
        let query = Key {
            components: path.as_path().components().collect(),
            kind: Kind::Note,
        };
        // `compare` reads a kind only where components diverge, so a row at exactly `path` is `Equal`.
        let at = self
            .state
            .entries
            .binary_search_by(|entry| compare(&Key::of(entry), &query))
            .ok()?;
        let entry = &self.state.entries[at];
        (entry.kind == Kind::Note).then_some(entry.id)
    }

    /// Gives `other`'s note row, and its inode, the id `id`; refuses a folder, an absent row or a live `id`.
    #[must_use]
    pub(crate) fn reclaim(&mut self, id: EntryId, other: EntryId) -> bool {
        if id == EntryId::DETACHED || self.state.by_id.contains_key(&id) {
            return false;
        }
        let Some(&at) = self.state.by_id.get(&other) else {
            return false;
        };
        if self.state.entries[at].kind != Kind::Note {
            return false;
        }
        let state = Arc::make_mut(&mut self.state);
        state.by_id.remove(&other);
        state.by_id.insert(id, at);
        Arc::make_mut(&mut state.entries)[at].id = id;
        // A live id answers to its entry's inode alone: the gone inode a partial walk kept lets go of it.
        state.by_identity.retain(|_, mapped| *mapped != id);
        for mapped in state.by_identity.values_mut() {
            if *mapped == other {
                *mapped = id;
            }
        }
        true
    }
}
