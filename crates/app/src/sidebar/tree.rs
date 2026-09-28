use std::cmp::Ordering;
use std::sync::Arc;

use gpui::{Context, ScrollStrategy, Window};
use inkra_vault::{EntryId, Kind};

use super::{Row, SelectNext, SelectPrev, Sidebar};

impl Sidebar {
    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        let entries = self
            .notes
            .read(cx)
            .vault()
            .map_or_else(|| Arc::from([]), |vault| vault.read(cx).shared_entries());
        let target = self.editing.as_ref().and_then(|edit| edit.target);
        let target_at = self.visible.rewalk(entries, &self.expanded, target);
        // The field must end up on a row the list still draws: an unrendered
        // row is in no dispatch node, so gpui silently swallows every keystroke.
        if let Some(edit) = self.editing.as_mut() {
            let at = match edit.target {
                Some(_) => target_at,
                None => Some(edit.at.min(self.visible.len())),
            };
            match at {
                Some(at) => {
                    edit.at = at;
                    self.scroll.scroll_to_item(at, ScrollStrategy::Center);
                }
                None => self.editing = None,
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
        let next = match self.selected.and_then(|id| self.visible.index_of(id)) {
            Some(index) => index.saturating_add_signed(delta).min(last),
            None if delta > 0 => 0,
            None => last,
        };
        self.selected = Some(self.visible.entry(next).id);
        cx.notify();
    }

    /// Must be computed before the delete: afterwards, `id`'s row no longer exists.
    pub(super) fn after_deleting(&self, id: EntryId) -> Option<EntryId> {
        let index = self.visible.index_of(id)?;
        let neighbour = if index + 1 < self.visible.len() {
            index + 1
        } else {
            index.checked_sub(1)?
        };
        Some(self.visible.entry(neighbour).id)
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
        let listed = match self.editing.as_ref() {
            None => ix,
            Some(edit) if edit.target.is_some() => (ix != edit.at).then_some(ix)?,
            Some(edit) => match ix.cmp(&edit.at) {
                Ordering::Less => ix,
                Ordering::Equal => return None,
                Ordering::Greater => ix - 1,
            },
        };
        (listed < self.visible.len()).then(|| self.visible.row(listed))
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
