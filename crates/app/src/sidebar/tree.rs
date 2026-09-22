use gpui::{Context, ScrollStrategy, Window};
use inkra_vault::{EntryId, Kind};

use super::{Row, SelectNext, SelectPrev, Sidebar};

impl Sidebar {
    /// `RelPath::starts_with` is component-wise, so the skip past a collapsed
    /// directory never swallows the sibling `Projects.md`.
    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            // Unreachable today: the vault is fixed at construction.
            cx.notify();
            return;
        };
        // Collected, not iterated: the walk borrows `cx`, which `notify` needs.
        let entries: Vec<_> = vault.read(cx).entries().cloned().collect();

        let mut i = 0;
        while i < entries.len() {
            let entry = &entries[i];
            self.visible.push(Row {
                id: entry.id,
                depth: entry.path.depth(),
                kind: entry.kind,
                name: entry.path.display_name(),
            });
            i += 1;
            if entry.kind == Kind::Dir && !self.expanded.contains(&entry.id) {
                while i < entries.len() && entries[i].path.starts_with(&entry.path) {
                    i += 1;
                }
            }
        }
        // A field open across this walk has to end up on a row the list still
        // draws: an unrendered row is in no dispatch node, so gpui falls back
        // to the root (`window.rs:6244-6251`) and every keystroke, `escape`
        // included, vanishes with no panic and no feedback.
        //
        // A rename follows its row by identity, so a walk that reordered the
        // tree keeps the field over the entry it names, and a target deleted
        // from outside ends the gesture. A create has no target and only needs
        // its index to stay in range.
        if let Some(target) = self.editing.as_ref().map(|edit| edit.target) {
            let at = match target {
                Some(id) => self.index_of(id),
                None => self
                    .editing
                    .as_ref()
                    .map(|edit| edit.at.min(self.visible.len())),
            };
            match (at, self.editing.as_mut()) {
                (Some(at), Some(edit)) => {
                    edit.at = at;
                    self.scroll.scroll_to_item(at, ScrollStrategy::Center);
                }
                _ => self.editing = None,
            }
        }
        cx.notify();
    }

    pub(super) fn toggle(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(cx);
    }

    /// Expansion is the sidebar's own state; where a note opens is not. The
    /// sidebar does not know an editor exists.
    pub(super) fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.selected = Some(row.id);
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(super::NoteActivated(row.id));
    }

    /// Moves the selection by one visible row, clamped at both ends rather than
    /// wrapping: a tree that jumps from the last note back to the first loses
    /// the user's place.
    ///
    /// Does nothing while a name is being typed. The field is a child of this
    /// view's dispatch node and claims neither arrow, so without the guard the
    /// key bubbles up here and moves the selection out from under the gesture
    /// in flight. `trash` and `rename` carry the same guard, and `cmd-backspace`
    /// is the one that bites: it is the macOS delete-to-line-start gesture, so
    /// a user correcting a name would be asked whether to delete the row.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.editing.is_some() || self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let next = match self.selected.and_then(|id| self.index_of(id)) {
            Some(index) => index.saturating_add_signed(delta).min(last),
            // Nothing selected: down starts at the top, up at the bottom.
            None if delta > 0 => 0,
            None => last,
        };
        self.selected = self.visible.get(next).map(|row| row.id);
        cx.notify();
    }

    pub(super) fn index_of(&self, id: EntryId) -> Option<usize> {
        self.visible.iter().position(|row| row.id == id)
    }

    /// Where the selection lands once `id` is gone: the next row, else the one
    /// before it, else nothing. Computed before the delete, because afterwards
    /// the row it is relative to no longer exists.
    pub(super) fn after_deleting(&self, id: EntryId) -> Option<EntryId> {
        let index = self.index_of(id)?;
        self.visible
            .get(index + 1)
            .or_else(|| {
                index
                    .checked_sub(1)
                    .and_then(|before| self.visible.get(before))
            })
            .map(|row| row.id)
    }

    /// The folder a row lives in: the nearest entry above it one level shallower.
    /// `None` for a top-level row, which is the vault root.
    pub(super) fn parent_of(&self, row: &Row) -> Option<EntryId> {
        if row.depth == 0 {
            return None;
        }
        self.visible[..self.index_of(row.id)?]
            .iter()
            .rev()
            .find(|above| above.depth == row.depth - 1 && above.kind == Kind::Dir)
            .map(|above| above.id)
    }

    pub(super) fn select_prev(
        &mut self,
        _: &SelectPrev,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(-1, cx);
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(1, cx);
    }

    /// One row per visible entry, plus the field's when a name is being typed.
    ///
    /// The field occupies an index of its own rather than replacing a row, so
    /// creating never hides the entry it was started from.
    pub(super) fn row_at(&self, ix: usize) -> Option<Row> {
        let Some(edit) = self.editing.as_ref() else {
            return self.visible.get(ix).cloned();
        };
        // A rename replaces its row; a create inserts one. Otherwise renaming
        // would show the old name and the field for one entry at once.
        if edit.target.is_some() {
            return (ix != edit.at)
                .then(|| self.visible.get(ix).cloned())
                .flatten();
        }
        match ix.cmp(&edit.at) {
            std::cmp::Ordering::Less => self.visible.get(ix).cloned(),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => self.visible.get(ix - 1).cloned(),
        }
    }

    /// How many rows the list draws: the tree, plus one for a field that is
    /// inserting rather than replacing.
    /// `pub(crate)` for the rename test in `shell.rs`, which asserts the count
    /// does not move: a rename replaces its row rather than adding one.
    pub(crate) fn row_count(&self) -> usize {
        let inserting = self
            .editing
            .as_ref()
            .is_some_and(|edit| edit.target.is_none());
        self.visible.len() + usize::from(inserting)
    }
}
