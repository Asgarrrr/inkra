//! The binding between files and buffers. Handles are **strong** — a weak
//! one would drop an unsaved buffer the moment the editor switches away.

use gpui::actions;

mod events;
mod gone;
mod ops;
mod reload;
mod save;
mod scans;
mod store;
mod tree_ops;
mod watch;
mod writer;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

actions!(vault, [Save]);

pub(crate) use events::{
    ConflictPreserved, GoneChanged, NoteClosed, NoteSaved, RenameFailed, SaveFailed, TrashFailed,
};
#[cfg(test)]
pub(crate) use store::AUTOSAVE_DELAY;
pub(crate) use store::NoteStore;
