use std::sync::Arc;

use gpui::{Context, ScrollStrategy, Window};
use inkra_vault::{EntryId, Kind};

use super::{Row, SelectNext, SelectPrev, Sidebar};
use crate::open::{NoteActivated, Open};

impl Sidebar {
    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        let entries = self
            .notes
            .read(cx)
            .vault()
            .map_or_else(|| Arc::from([]), |vault| vault.read(cx).shared_entries());
        // The field must end up on a row the list still draws: an unrendered
        // row is in no dispatch node, so gpui silently swallows every keystroke.
        if let Some(item) = self.rows.rewalk(entries, &self.expanded) {
            self.scroll.scroll_to_item(item, ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub(super) fn toggle(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(cx);
    }

    /// Guarded while a name is typed, as `step`: the field keeps its row.
    pub(super) fn activate(&mut self, row: &Row, open: Open, cx: &mut Context<Self>) {
        if self.rows.edit().is_some() {
            return;
        }
        self.selected = Some(row.id);
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(NoteActivated { note: row.id, open });
    }

    /// Guarded while a name is typed, or an arrow key bubbles up and moves
    /// the selection out from under the field. `trash` and `rename` carry
    /// the same guard.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.rows.edit().is_some() {
            return;
        }
        let Some(next) = self.rows.visible().step(self.selected, delta) else {
            return;
        };
        self.selected = Some(next);
        cx.notify();
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

    /// `pub(crate)` for the rename test in `shell.rs`, which asserts the count does not move.
    pub(crate) fn item_count(&self) -> usize {
        self.rows.item_count()
    }
}
