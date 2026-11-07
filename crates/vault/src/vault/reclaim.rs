//! An open note gone from disk takes back a file at its last path (watcher spec, F9).

use std::sync::Arc;

use super::order::position;
use super::{EntryId, Vault};
use crate::RelPath;
use crate::fs::Kind;

impl Vault {
    /// The note at `path`; `None` for a folder, whatever its name.
    #[must_use]
    pub(crate) fn id_at(&self, path: &RelPath) -> Option<EntryId> {
        let at = position(&self.state.entries, path, Kind::Note)?;
        Some(self.state.entries[at].id)
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
        state.departures.renew();
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
