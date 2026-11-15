//! Our own renames ahead of their rescan: the paths the vault does not list yet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use inkra_vault::{EntryId, Vault};

#[cfg(test)]
mod tests;

/// Per entry, until its rename's rescan swaps in.
#[derive(Default)]
pub(super) struct Moves(HashMap<EntryId, Move>);

enum Move {
    /// `Fs::rename` runs: no path is safe.
    Running,
    Landed(PathBuf),
}

/// The entry a `begin` replaced, for `fail` to put back.
#[must_use]
pub(super) struct Begun(Option<Move>);

impl Moves {
    /// `vault`'s path unless a rename is ahead of it; `None` while one runs.
    pub(super) fn resolve(&self, id: EntryId, vault: &Vault) -> Option<PathBuf> {
        match self.0.get(&id) {
            Some(Move::Running) => None,
            Some(Move::Landed(moved)) => Some(moved.clone()),
            None => vault.absolute(id),
        }
    }

    /// The path under the vault root; a rename still running answers the vault's.
    pub(super) fn relative(&self, id: EntryId, vault: &Vault) -> Option<PathBuf> {
        match self.0.get(&id) {
            Some(Move::Landed(moved)) => moved.strip_prefix(vault.root()).ok().map(PathBuf::from),
            Some(Move::Running) | None => Some(vault.entry(id)?.path.as_path().to_path_buf()),
        }
    }

    pub(super) fn begin(&mut self, id: EntryId) -> Begun {
        Begun(self.0.insert(id, Move::Running))
    }

    pub(super) fn fail(&mut self, id: EntryId, begun: Begun) {
        match begun.0 {
            Some(before) => self.0.insert(id, before),
            None => self.0.remove(&id),
        };
    }

    pub(super) fn moved(&mut self, id: EntryId, to: PathBuf) {
        self.0.insert(id, Move::Landed(to));
    }

    /// Only while the entry is still `to`: a newer rename's move stays.
    pub(super) fn settle(&mut self, id: EntryId, to: &Path) {
        if matches!(self.0.get(&id), Some(Move::Landed(moved)) if moved == to) {
            self.0.remove(&id);
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
