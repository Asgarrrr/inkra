use std::path::PathBuf;

use gpui::EventEmitter;
use inkra_vault::EntryId;

use super::NoteStore;

/// A write failed. Emitted rather than printed so the deferred status line has
/// one seam to subscribe to instead of two call sites that grew apart — and so
/// the sidebar can tell a note whose last save failed from one being typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SaveFailed(pub(crate) EntryId);

impl EventEmitter<SaveFailed> for NoteStore {}

/// A write found the file changed underneath it and preserved the other
/// version beside the note. Emitted for the same reason as `SaveFailed`: the
/// deferred status line gets one seam to subscribe to rather than two that grew
/// apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictPreserved {
    pub(crate) id: EntryId,
    pub(crate) path: PathBuf,
}

impl EventEmitter<ConflictPreserved> for NoteStore {}

/// The note is gone from the vault, and whatever was showing it must stop.
///
/// Fired by `trash` alone. Buffer eviction, when it lands, is a different
/// signal on the same axis and must not reuse this one: an evicted note is not
/// necessarily the note on screen, so a subscriber that blanks the editor would
/// be wrong for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteClosed(pub(crate) EntryId);

impl EventEmitter<NoteClosed> for NoteStore {}

/// A delete failed, and the note is still there. The counterpart to
/// `SaveFailed`, and emitted for the same reason: one seam for the deferred
/// status line rather than two call sites that grow apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrashFailed(pub(crate) EntryId);

impl EventEmitter<TrashFailed> for NoteStore {}

/// A rename failed. The entry is where it was, under the name it had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenameFailed(pub(crate) EntryId);

impl EventEmitter<RenameFailed> for NoteStore {}
