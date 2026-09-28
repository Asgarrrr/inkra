//! A note deleted on disk stays open; a note file at its last path brings it back (watcher spec, F9).

use inkra_vault::{EntryId, RelPath, Vault};

use super::NoteStore;
use super::store::Location;

impl NoteStore {
    /// Before the swap's replay, on the state it swaps in: each open note with no entry takes back
    /// the note at its last path, unless that id is open. In id order, so two claims resolve alike.
    /// Whether one did.
    pub(super) fn reclaim_gone(&self, next: &mut Vault, live: &Vault) -> bool {
        let mut gone: Vec<EntryId> = self
            .open
            .keys()
            .copied()
            .filter(|id| *id != EntryId::DETACHED && next.entry(*id).is_none())
            .filter(|id| !self.ops.get(id).is_some_and(|ops| ops.trashing))
            .collect();
        gone.sort_unstable();
        let mut reclaimed = false;
        for id in gone {
            let path = match self.open.get(&id).map(|note| &note.location) {
                Some(Location::Gone(path)) => Some(path.clone()),
                _ => live.absolute(id),
            };
            let Some(path) =
                path.and_then(|path| RelPath::new(path.strip_prefix(next.root()).ok()?))
            else {
                continue;
            };
            // Open, or reclaimed earlier in this pass: its note keeps it.
            if let Some(other) = next
                .id_at(&path)
                .filter(|other| !self.open.contains_key(other))
            {
                reclaimed |= next.reclaim(id, other);
            }
        }
        reclaimed
    }
}
