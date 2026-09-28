//! A note deleted on disk stays open; a note file at its last path brings it back (watcher spec, F9).

use inkra_vault::Claims;

use super::NoteStore;
use super::store::Location;

impl NoteStore {
    /// What the open notes hold at a swap: every open note but one being trashed claims its last path.
    pub(super) fn claims(&self) -> Claims {
        let claimants = self
            .open
            .iter()
            .filter(|(id, _)| !self.ops.get(id).is_some_and(|ops| ops.trashing))
            .map(|(id, note)| {
                let kept = match &note.location {
                    Location::Gone(path) => Some(path.clone()),
                    Location::Vault | Location::Detached(_) => None,
                };
                (*id, kept)
            })
            .collect();
        Claims {
            open: self.open.keys().copied().collect(),
            claimants,
        }
    }
}
