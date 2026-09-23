use std::path::PathBuf;

use gpui::EventEmitter;
use inkra_vault::EntryId;

use super::NoteStore;

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
