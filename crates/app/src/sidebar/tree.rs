use gpui::{Context, ScrollStrategy, Window};
use inkra_vault::{EntryId, Kind};

use super::{Row, SelectNext, SelectPrev, Sidebar};

impl Sidebar {
    /// `RelPath::starts_with` is component-wise, so skipping a collapsed
    /// directory never swallows the sibling `Projects.md`.
    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            cx.notify();
            return;
        };
        let entries = vault.read(cx).shared_entries();

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
        // The field must end up on a row the list still draws: an unrendered
        // row is in no dispatch node, so gpui silently swallows every keystroke.
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

    pub(super) fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.selected = Some(row.id);
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(super::NoteActivated(row.id));
    }

    /// Guarded while a name is typed, or an arrow key bubbles up and moves
    /// the selection out from under the field. `trash` and `rename` carry
    /// the same guard.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.editing.is_some() || self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let next = match self.selected.and_then(|id| self.index_of(id)) {
            Some(index) => index.saturating_add_signed(delta).min(last),
            None if delta > 0 => 0,
            None => last,
        };
        self.selected = self.visible.get(next).map(|row| row.id);
        cx.notify();
    }

    pub(super) fn index_of(&self, id: EntryId) -> Option<usize> {
        self.visible.iter().position(|row| row.id == id)
    }

    /// Must be computed before the delete: afterwards, `id`'s row no longer exists.
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

    /// A create occupies an index of its own; a rename replaces its row —
    /// otherwise renaming would show the old name and the field at once.
    pub(super) fn row_at(&self, ix: usize) -> Option<Row> {
        let Some(edit) = self.editing.as_ref() else {
            return self.visible.get(ix).cloned();
        };
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

    /// `pub(crate)` for the rename test in `shell.rs`, which asserts the count does not move.
    pub(crate) fn row_count(&self) -> usize {
        let inserting = self
            .editing
            .as_ref()
            .is_some_and(|edit| edit.target.is_none());
        self.visible.len() + usize::from(inserting)
    }
}
