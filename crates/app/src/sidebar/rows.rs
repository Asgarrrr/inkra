use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

use inkra_vault::{Entry, EntryId};

use super::visible::Visible;

/// The list `uniform_list` draws: the tree's rows plus at most one name field.
/// An item indexes this list, a row indexes `Visible`.
pub(super) struct Rows<E> {
    visible: Visible,
    slot: Option<Slot<E>>,
}

struct Slot<E> {
    item: usize,
    place: Place,
    edit: E,
}

#[derive(Clone, Copy)]
enum Place {
    /// A create: the field shifts the rows below it.
    Insert,
    /// A rename: the field takes its target's row.
    Replace(EntryId),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Item<'a, E> {
    Row(usize),
    Field(&'a E),
}

impl<E> Default for Rows<E> {
    fn default() -> Self {
        Self {
            visible: Visible::default(),
            slot: None,
        }
    }
}

impl<E> Rows<E> {
    /// Walks, then re-places the field or closes it; returns the field's item.
    /// The target is found inside the walk (rebuild spec S3), never by a second pass.
    pub(super) fn rewalk(
        &mut self,
        entries: Arc<[Entry]>,
        expanded: &HashSet<EntryId>,
    ) -> Option<usize> {
        let target = self.target();
        let found = self.visible.rewalk(entries, expanded, target);
        let slot = self.slot.as_mut()?;
        let item = match slot.place {
            Place::Replace(_) => found,
            Place::Insert => Some(slot.item.min(self.visible.len())),
        };
        match item {
            Some(item) => slot.item = item,
            None => self.slot = None,
        }
        item
    }

    pub(super) fn visible(&self) -> &Visible {
        &self.visible
    }

    pub(super) fn item_count(&self) -> usize {
        let inserting = self
            .slot
            .as_ref()
            .is_some_and(|slot| matches!(slot.place, Place::Insert));
        self.visible.len() + usize::from(inserting)
    }

    pub(super) fn item(&self, item: usize) -> Option<Item<'_, E>> {
        let row = match &self.slot {
            None => item,
            Some(slot) => match (item.cmp(&slot.item), slot.place) {
                (Ordering::Equal, _) => return Some(Item::Field(&slot.edit)),
                (Ordering::Greater, Place::Insert) => item - 1,
                _ => item,
            },
        };
        (row < self.visible.len()).then_some(Item::Row(row))
    }

    /// Opens a create's field at `item`. Panics past the row count: a caller bug.
    pub(super) fn insert(&mut self, item: usize, edit: E) {
        assert!(item <= self.visible.len(), "a field past the list's end");
        self.slot = Some(Slot {
            item,
            place: Place::Insert,
            edit,
        });
    }

    /// Opens a rename's field on `target`'s row; `None`, and no field, when it has no row.
    pub(super) fn replace(&mut self, target: EntryId, edit: E) -> Option<usize> {
        let item = self.visible.index_of(target)?;
        self.slot = Some(Slot {
            item,
            place: Place::Replace(target),
            edit,
        });
        Some(item)
    }

    pub(super) fn close(&mut self) -> Option<E> {
        self.slot.take().map(|slot| slot.edit)
    }

    pub(super) fn edit(&self) -> Option<&E> {
        self.slot.as_ref().map(|slot| &slot.edit)
    }

    pub(super) fn edit_mut(&mut self) -> Option<&mut E> {
        self.slot.as_mut().map(|slot| &mut slot.edit)
    }

    pub(super) fn target(&self) -> Option<EntryId> {
        match self.slot.as_ref()?.place {
            Place::Insert => None,
            Place::Replace(target) => Some(target),
        }
    }

    #[cfg(test)]
    pub(super) fn field_item(&self) -> Option<usize> {
        self.slot.as_ref().map(|slot| slot.item)
    }
}
