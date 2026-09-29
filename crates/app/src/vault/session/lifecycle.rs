//! One id's lifecycle in the store: where its file is, and whether our trash owns it. No gpui, no buffer.

use std::mem;
use std::path::PathBuf;

use inkra_vault::Mtime;

use super::Folder;

/// Where the note's file is; only a note outside the vault's entries carries a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vault) enum Location {
    /// Resolved against the vault at write time: a rename never updates the note.
    Vault,
    /// `EntryId::DETACHED`, a file opened outside any vault.
    Detached(PathBuf),
    /// The file left the vault; the path is its last vault path (watcher spec, F9).
    Gone(PathBuf),
}

impl Location {
    pub(in crate::vault) fn folder(&self) -> Folder {
        match self {
            Self::Gone(_) => Folder::Create,
            Self::Vault | Self::Detached(_) => Folder::MustExist,
        }
    }
}

/// `B` is the bound note: the store's `OpenNote`, `()` in the tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::vault) enum Lifecycle<B> {
    #[default]
    Closed,
    Open(Location, B),
    /// Our trash is queued or done; `Some` while the note is bound. Sticky once done: a second trash is refused.
    Trashing(Option<(Location, B)>),
}

/// What a queued operation does to the note's pending save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vault) enum PendingSave {
    /// Written now, ahead of the operation: cancelling would lose the text.
    Write,
    /// Cancelled: the write would recreate a file the operation discards.
    Drop,
    /// Left alone: a reload runs on clean notes only.
    Keep,
}

/// What one swap did to one bound note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vault) struct Swap {
    pub(in crate::vault) flipped: bool,
    pub(in crate::vault) reload: bool,
}

impl Swap {
    pub(in crate::vault) const NONE: Self = Self {
        flipped: false,
        reload: false,
    };
}

impl<B> Lifecycle<B> {
    /// A load that lands during our trash opens trashing.
    pub(in crate::vault) fn open(&mut self, location: Location, bound: B) {
        *self = match mem::replace(self, Self::Closed) {
            Self::Closed | Self::Open(..) => Self::Open(location, bound),
            Self::Trashing(_) => Self::Trashing(Some((location, bound))),
        };
    }

    pub(in crate::vault) fn bound(&self) -> Option<&B> {
        self.bound_at().map(|(_, bound)| bound)
    }

    pub(in crate::vault) fn bound_mut(&mut self) -> Option<&mut B> {
        match self {
            Self::Open(_, bound) | Self::Trashing(Some((_, bound))) => Some(bound),
            Self::Closed | Self::Trashing(None) => None,
        }
    }

    pub(in crate::vault) fn location(&self) -> Option<&Location> {
        self.bound_at().map(|(location, _)| location)
    }

    pub(in crate::vault) fn is_gone(&self) -> bool {
        self.gone_path().is_some()
    }

    /// The last vault path a gone note keeps; its claim at a swap.
    pub(in crate::vault) fn gone_path(&self) -> Option<&PathBuf> {
        match self.location()? {
            Location::Gone(path) => Some(path),
            Location::Vault | Location::Detached(_) => None,
        }
    }

    pub(in crate::vault) fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// The quit flush, a reload and a swap's claims act on this alone: a trashing note is our trash's.
    pub(in crate::vault) fn is_open(&self) -> bool {
        matches!(self, Self::Open(..))
    }

    /// After a swap (watcher spec, F7 and F9): `now` is the live entry's mtime, `before` the replaced
    /// state's, `last` the replaced state's path. A trashing note still flips back: its own save's re-key
    /// can return the entry mid-trash.
    pub(in crate::vault) fn swapped(
        &mut self,
        now: Option<Mtime>,
        before: Option<Mtime>,
        last: impl FnOnce() -> Option<PathBuf>,
    ) -> Swap {
        let trashing = matches!(self, Self::Trashing(_));
        let (Self::Open(location, _) | Self::Trashing(Some((location, _)))) = self else {
            return Swap::NONE;
        };
        match (now, &*location) {
            (_, Location::Detached(_)) | (None, Location::Gone(_)) => Swap::NONE,
            // Our own trash closes the note; it is never gone.
            (None, Location::Vault) if trashing => Swap::NONE,
            (None, Location::Vault) => match last() {
                Some(path) => {
                    *location = Location::Gone(path);
                    Swap {
                        flipped: true,
                        reload: false,
                    }
                }
                None => Swap::NONE,
            },
            (Some(now), _) => {
                let flipped = matches!(location, Location::Gone(_));
                *location = Location::Vault;
                Swap {
                    flipped,
                    reload: before != Some(now),
                }
            }
        }
    }

    /// `None` refuses the trash; else the note is trashing, and the answer says what to do with its
    /// pending save. A gone note needs no entry: it closes, trashing only a file its own write put back.
    pub(in crate::vault) fn begin_trash(&mut self, has_entry: bool) -> Option<PendingSave> {
        let pending = match self {
            Self::Open(Location::Gone(_), _) => PendingSave::Drop,
            Self::Open(Location::Vault, _) | Self::Closed if has_entry => PendingSave::Write,
            Self::Open(..) | Self::Closed | Self::Trashing(_) => return None,
        };
        *self = match mem::replace(self, Self::Closed) {
            Self::Open(location, bound) => Self::Trashing(Some((location, bound))),
            Self::Closed | Self::Trashing(_) => Self::Trashing(None),
        };
        Some(pending)
    }

    pub(in crate::vault) fn trash_failed(&mut self) {
        let Self::Trashing(bound) = self else {
            return;
        };
        *self = match bound.take() {
            Some((location, bound)) => Self::Open(location, bound),
            None => Self::Closed,
        };
    }

    /// Hands back the bound note, so it closes inside the caller's update.
    pub(in crate::vault) fn trash_done(&mut self) -> Option<B> {
        let Self::Trashing(bound) = self else {
            return None;
        };
        bound.take().map(|(_, bound)| bound)
    }

    /// A trashing gone note's last path: with no entry, the one file on disk that may be its own.
    pub(in crate::vault) fn last_write(&self) -> Option<(&PathBuf, &B)> {
        match self {
            Self::Trashing(Some((Location::Gone(path), bound))) => Some((path, bound)),
            Self::Closed | Self::Open(..) | Self::Trashing(_) => None,
        }
    }

    /// The bound note with its location: the two exist together or not at all.
    pub(in crate::vault) fn bound_at(&self) -> Option<(&Location, &B)> {
        match self {
            Self::Open(location, bound) | Self::Trashing(Some((location, bound))) => {
                Some((location, bound))
            }
            Self::Closed | Self::Trashing(None) => None,
        }
    }
}

#[cfg(test)]
mod tests;
