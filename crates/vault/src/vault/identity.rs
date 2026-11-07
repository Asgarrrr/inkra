//! Which id each row of a walk takes (watcher spec, F5).

use std::collections::{HashMap, HashSet};

use super::order::position;
use super::{Entry, EntryId, Row, Vault};
use crate::RelPath;
use crate::fs::FileIdentity;

impl Vault {
    /// Gives each found row its id, in path order; returns the rows as entries and the next id to mint.
    pub(super) fn mint(
        &self,
        rows: Vec<Row>,
        mut unclaimed: HashSet<FileIdentity>,
    ) -> (Vec<(Entry, FileIdentity)>, u32) {
        let state = &self.state;
        let replaced = self.replaced(&rows);

        let mut next_id = state.next_id;
        let entries = rows
            .into_iter()
            .filter_map(|row| {
                let id = match row.id {
                    Some(id) => id,
                    // Hard links collapse to the path that sorts first.
                    None if !unclaimed.remove(&row.identity) => return None,
                    None => {
                        let known = state.by_identity.get(&row.identity);
                        let taken = known.or_else(|| replaced.get(&row.path)).copied();
                        taken.unwrap_or_else(|| {
                            let id = EntryId(next_id);
                            next_id += 1;
                            id
                        })
                    }
                };
                let entry = Entry {
                    id,
                    path: row.path,
                    kind: row.kind,
                    mtime: row.mtime,
                    size: row.size,
                    identity: row.identity,
                };
                Some((entry, row.identity))
            })
            .collect();
        (entries, next_id)
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
    fn replaced(&self, rows: &[Row]) -> HashMap<RelPath, EntryId> {
        let state = &self.state;
        let found = || rows.iter().filter(|row| row.id.is_none());
        let mut replaced: HashMap<RelPath, EntryId> = found()
            .filter(|row| !state.by_identity.contains_key(&row.identity))
            .filter_map(|row| {
                let at = position(&state.entries, &row.path, row.kind)?;
                Some((row.path.clone(), state.entries[at].id))
            })
            .collect();
        if !replaced.is_empty() {
            let claimed: HashSet<EntryId> = found()
                .filter_map(|row| state.by_identity.get(&row.identity).copied())
                .collect();
            replaced.retain(|_, id| !claimed.contains(id));
        }
        replaced
    }
}
