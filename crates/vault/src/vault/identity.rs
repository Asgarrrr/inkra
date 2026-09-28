//! Which id each row of a walk takes (watcher spec, F5).

use std::collections::{HashMap, HashSet};

use super::{Entry, EntryId, Key, Vault, compare};
use crate::RelPath;
use crate::fs::FileIdentity;

impl Vault {
    /// Gives each found row its id, in path order; returns the next id to mint.
    pub(super) fn mint(
        &self,
        rows: &mut Vec<(Entry, FileIdentity)>,
        mut unclaimed: HashSet<FileIdentity>,
    ) -> u32 {
        let state = &self.state;
        let replaced = self.replaced(rows);

        let mut next_id = state.next_id;
        // A found row holds `DETACHED` until minted; a kept entry never does.
        rows.retain_mut(|(entry, identity)| {
            if entry.id != EntryId::DETACHED {
                return true;
            }
            // Hard links collapse to the path that sorts first.
            if !unclaimed.remove(identity) {
                return false;
            }
            let known = state.by_identity.get(identity);
            let taken = known.or_else(|| replaced.get(&entry.path)).copied();
            entry.id = taken.unwrap_or_else(|| {
                let id = EntryId(next_id);
                next_id += 1;
                id
            });
            true
        });
        next_id
    }

    /// Keeps the identities of ids no entry holds, for a walk that is not whole.
    pub(super) fn carry_over(
        &self,
        by_identity: &mut HashMap<FileIdentity, EntryId>,
        by_id: &HashMap<EntryId, usize>,
    ) {
        // A live id answers to its entry's inode alone: a replaced inode lets go of it.
        let unheld = self
            .state
            .by_identity
            .iter()
            .filter(|(_, id)| !by_id.contains_key(id));
        for (identity, id) in unheld {
            by_identity.entry(*identity).or_insert(*id);
        }
    }

    /// Ids found files take over: a new inode at a path whose old inode this walk found nowhere (watcher spec, F5).
    fn replaced(&self, rows: &[(Entry, FileIdentity)]) -> HashMap<RelPath, EntryId> {
        let state = &self.state;
        let found = || {
            rows.iter()
                .filter(|(entry, _)| entry.id == EntryId::DETACHED)
        };
        let mut replaced: HashMap<RelPath, EntryId> = found()
            .filter(|(_, identity)| !state.by_identity.contains_key(identity))
            .filter_map(|(entry, _)| {
                let key = Key::of(entry);
                let at = state
                    .entries
                    .binary_search_by(|old| compare(&Key::of(old), &key))
                    .ok()?;
                let old = &state.entries[at];
                (old.kind == entry.kind).then(|| (entry.path.clone(), old.id))
            })
            .collect();
        if !replaced.is_empty() {
            let claimed: HashSet<EntryId> = found()
                .filter_map(|(_, identity)| state.by_identity.get(identity).copied())
                .collect();
            replaced.retain(|_, id| !claimed.contains(id));
        }
        replaced
    }
}
