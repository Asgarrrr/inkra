use gpui::{Context, ScrollStrategy, Window};
use inkra_vault::{EntryId, Kind};

use super::{Cursor, Row, SelectNext, SelectPrev, Sidebar};
use crate::open::{NoteActivated, Open};

impl Sidebar {
    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        let entries = self.vault.read(cx).shared_entries();
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
        self.select(Some(row.id));
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(NoteActivated { note: row.id, open });
    }

    /// Every tree write of `selected`: a landing begun before it must not undo it (`follow`).
    pub(super) fn select(&mut self, id: Option<EntryId>) {
        self.selected = id;
        self.cursor.0 += 1;
    }

    pub(crate) fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Moves `selected` to the note showing, unless the tree wrote it since `since` or a name is typed.
    pub(crate) fn follow(&mut self, id: EntryId, since: Cursor, cx: &mut Context<Self>) {
        if self.rows.edit().is_some() || self.cursor != since || self.selected == Some(id) {
            return;
        }
        self.selected = Some(id);
        cx.notify();
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
        self.select(Some(next));
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
