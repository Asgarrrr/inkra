use std::path::PathBuf;
use std::sync::Arc;

use gpui::EventEmitter;
use inkra_vault::{EntryId, RelPath, Stamp, VaultError};

use super::NoteStore;

/// A write landed; its one subscriber, `IndexSync`, exists only beside a vault, so never beside a detached note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteSaved {
    pub(crate) id: EntryId,
    pub(crate) text: Arc<str>,
    pub(crate) stamp: Stamp,
}

impl EventEmitter<NoteSaved> for NoteStore {}

/// A write failed; `error` is `None` when the note has no path in the vault.
#[derive(Debug)]
pub(crate) struct SaveFailed {
    pub(crate) id: EntryId,
    pub(crate) error: Option<VaultError>,
}

impl EventEmitter<SaveFailed> for NoteStore {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictPreserved {
    pub(crate) id: EntryId,
    pub(crate) path: PathBuf,
}

impl EventEmitter<ConflictPreserved> for NoteStore {}

/// Must not be reused for buffer eviction: an evicted note isn't
/// necessarily the one on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteClosed(pub(crate) EntryId);

impl EventEmitter<NoteClosed> for NoteStore {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrashFailed(pub(crate) EntryId);

impl EventEmitter<TrashFailed> for NoteStore {}

#[derive(Debug)]
pub(crate) struct RenameFailed {
    pub(crate) id: EntryId,
    pub(crate) error: VaultError,
}

impl EventEmitter<RenameFailed> for NoteStore {}

/// A note's load failed; the tab stays as it was.
#[derive(Debug)]
pub(crate) struct OpenFailed {
    pub(crate) id: EntryId,
    pub(crate) error: VaultError,
}

impl EventEmitter<OpenFailed> for NoteStore {}

/// The note's file left the vault, or came back (watcher spec, F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GoneChanged(pub(crate) EntryId);

impl EventEmitter<GoneChanged> for NoteStore {}

/// Paths under the vault's `CONFIG_DIR` changed, once per batch of the watch's events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigChanged(pub(crate) Vec<RelPath>);

impl EventEmitter<ConfigChanged> for NoteStore {}
