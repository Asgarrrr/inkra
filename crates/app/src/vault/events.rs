use std::path::PathBuf;
use std::sync::Arc;

use gpui::EventEmitter;
use inkra_vault::{EntryId, Stamp};

use super::NoteStore;

/// A write landed; never emitted for `EntryId::DETACHED`, which no vault indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteSaved {
    pub(crate) id: EntryId,
    pub(crate) text: Arc<str>,
    pub(crate) stamp: Stamp,
}

impl EventEmitter<NoteSaved> for NoteStore {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SaveFailed(pub(crate) EntryId);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenameFailed(pub(crate) EntryId);

impl EventEmitter<RenameFailed> for NoteStore {}
