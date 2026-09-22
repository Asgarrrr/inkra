//! The binding between files and buffers. Handles are **strong**: weak ones
//! dropped an unsaved buffer the moment the editor switched away, losing the
//! text and the undo history with no prompt (critical defect 2 in
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`).

use gpui::actions;

mod events;
mod ops;
mod store;
mod tree_ops;
mod writer;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

actions!(vault, [Save]);

pub(crate) use events::{ConflictPreserved, NoteClosed, RenameFailed, SaveFailed, TrashFailed};
pub(crate) use store::NoteStore;
