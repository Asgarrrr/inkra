//! The binding between files and buffers. Handles are **strong** — a weak
//! one would drop an unsaved buffer the moment the editor switches away.

use gpui::actions;

mod events;
mod reload;
mod save;
mod scans;
mod session;
mod store;
mod tree_ops;
mod watch;

#[cfg(test)]
mod tests;

actions!(vault, [Save]);

pub(crate) use events::{
    ConflictPreserved, GoneChanged, NoteClosed, NoteSaved, OpenFailed, RenameFailed, SaveFailed,
    TrashFailed,
};
#[cfg(any(test, feature = "shot"))]
pub(crate) use store::AUTOSAVE_DELAY;
pub(crate) use store::NoteStore;
