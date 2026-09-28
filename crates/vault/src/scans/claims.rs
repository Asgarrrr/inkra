//! An open note gone from disk takes back the note file at its last path (watcher spec, F9).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use crate::RelPath;
use crate::vault::{EntryId, Vault};

/// What the open notes hold at a swap.
#[derive(Debug, Default)]
pub struct Claims {
    /// Every open id: a reclaim never takes one from its note.
    pub open: HashSet<EntryId>,
    /// Notes that may take back the note at their last path: the path the caller keeps for a
    /// gone note, else `None` for the live vault's.
    pub claimants: BTreeMap<EntryId, Option<PathBuf>>,
}

impl Claims {
    /// Before the swap's replay, on the state it swaps in: each claimant with no entry takes back the
    /// note at its last path, unless an open note holds it. In id order, so two claims resolve alike.
    /// Whether one did.
    pub(super) fn reclaim(&self, next: &mut Vault, live: &Vault) -> bool {
        let mut reclaimed = false;
        for (&id, kept) in &self.claimants {
            // A detached note has no last path in the vault to take back.
            if id == EntryId::DETACHED {
                continue;
            }
            if next.entry(id).is_some() {
                continue;
            }
            // The live vault's: the new state has no entry for a gone id.
            let last = kept.clone().or_else(|| live.absolute(id));
            let Some(path) =
                last.and_then(|path| RelPath::new(path.strip_prefix(next.root()).ok()?))
            else {
                continue;
            };
            // Open, or reclaimed earlier in this pass: its note keeps it.
            if let Some(other) = next.id_at(&path).filter(|other| !self.open.contains(other)) {
                reclaimed |= next.reclaim(id, other);
            }
        }
        reclaimed
    }
}
