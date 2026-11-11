use std::collections::HashSet;
use std::sync::Arc;

use inkra_vault::{Entry, EntryId};

use super::Row;

/// The drawn rows: positions into one vault snapshot. Only `rewalk` sets
/// either field, and always both, so a position never outlives its snapshot.
#[derive(Default)]
pub(super) struct Visible {
    entries: Arc<[Entry]>,
    positions: Vec<usize>,
}

impl Visible {
    /// Walks `entries` and returns where `target` landed. Cost: one step per
    /// drawn folder, one binary search per folder, one copy per run of files —
    /// never one step per hidden entry.
    pub(super) fn rewalk(
        &mut self,
        entries: Arc<[Entry]>,
        expanded: &HashSet<EntryId>,
        target: Option<EntryId>,
    ) -> Option<usize> {
        let positions = &mut self.positions;
        positions.clear();
        positions.reserve(entries.len());
        let mut target_at = None;
        // The subtree ends of the expanded folders enclosing `position`, innermost last.
        let mut ends: Vec<usize> = Vec::new();
        let mut position = 0;
        while position < entries.len() {
            while ends.last().is_some_and(|&end| end <= position) {
                ends.pop();
            }
            let entry = &entries[position];
            let (run, next) = if entry.kind.is_file() {
                // Folders sort first at each level, so every entry up to the
                // enclosing folder's end is a file of this level.
                let end = ends.last().copied().unwrap_or(entries.len());
                debug_assert!(entries[position..end].iter().all(|e| e.kind.is_file()));
                (position..end, end)
            } else {
                let end = position + subtree_len(&entries[position..]);
                if expanded.contains(&entry.id) {
                    ends.push(end);
                    (position..position + 1, position + 1)
                } else {
                    (position..position + 1, end)
                }
            };
            if let Some(id) = target
                && let Some(found) = entries[run.clone()].iter().position(|e| e.id == id)
            {
                target_at = Some(positions.len() + found);
            }
            positions.extend(run);
            position = next;
        }
        self.entries = entries;
        target_at
    }

    pub(super) fn len(&self) -> usize {
        self.positions.len()
    }

    /// Panics past the end: `ix` is a row index, not an entry position.
    pub(super) fn entry(&self, ix: usize) -> &Entry {
        &self.entries[self.positions[ix]]
    }

    /// Allocates the name: for drawn rows and gestures, never for a walk.
    pub(super) fn row(&self, ix: usize) -> Row {
        let entry = self.entry(ix);
        Row {
            id: entry.id,
            depth: entry.path.depth(),
            kind: entry.kind,
            name: entry.display_name(),
        }
    }

    pub(super) fn index_of(&self, id: EntryId) -> Option<usize> {
        self.positions
            .iter()
            .position(|&position| self.entries[position].id == id)
    }

    /// The row that takes `id`'s place when it leaves: below, else above.
    /// Call before removing the row: after removal this returns `None`.
    pub(super) fn neighbour(&self, id: EntryId) -> Option<EntryId> {
        let ix = self.index_of(id)?;
        let neighbour = if ix + 1 < self.len() {
            ix + 1
        } else {
            ix.checked_sub(1)?
        };
        Some(self.entry(neighbour).id)
    }

    /// The row index one step from `from`; no selection enters at the first or last row.
    pub(super) fn step(&self, from: Option<EntryId>, delta: isize) -> Option<usize> {
        let last = self.len().checked_sub(1)?;
        let next = match from.and_then(|id| self.index_of(id)) {
            Some(ix) => ix.saturating_add_signed(delta).min(last),
            None if delta > 0 => 0,
            None => last,
        };
        Some(next)
    }

    /// The folder holding the row at `ix`: a folder's subtree follows it, so it is above.
    pub(super) fn parent_of(&self, ix: usize) -> Option<EntryId> {
        let path = &self.entry(ix).path;
        if path.depth() == 0 {
            return None;
        }
        let parent = path.as_path().parent()?;
        self.positions[..ix]
            .iter()
            .rev()
            .map(|&position| &self.entries[position])
            .find(|above| above.path.as_path() == parent)
            .map(|above| above.id)
    }
}

/// The length of the subtree `entries[0]` roots, itself included: the vault
/// keeps a folder's descendants contiguous right after it (vault spec, "Ordering").
/// `RelPath::starts_with` is component-wise, so the sibling `Projects.md` stays out.
fn subtree_len(entries: &[Entry]) -> usize {
    let root = &entries[0].path;
    let len = entries.partition_point(|entry| entry.path.starts_with(root));
    debug_assert!(
        len > 0
            && entries[len - 1].path.starts_with(root)
            && entries
                .get(len)
                .is_none_or(|next| !next.path.starts_with(root)),
        "the vault's order broke the contiguity of {root:?}'s subtree"
    );
    len
}
