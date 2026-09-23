//! The binding between files and buffers. Handles are **strong** — a weak
//! one would drop an unsaved buffer the moment the editor switches away.

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
