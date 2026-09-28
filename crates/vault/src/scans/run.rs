//! A scan's reads, off the foreground (open-first spec, O1 and O6).

use crate::RelPath;
use crate::fs::{Fs, Result};
use crate::vault::{Jobs, Reach, Vault, Walk};
use crate::watch::Pending;

/// A pass to run off the foreground: its base and what it reads. Only `Scans` makes one.
#[must_use = "only `Scans::swap` ends a scan; a dropped one leaves `Scans` running forever"]
pub struct Scan {
    base: Vault,
    read: Pending,
    pass: Pass,
}

enum Pass {
    /// No full scan has applied: the root level first, so the sidebar shows it (open-first spec, O6).
    Root,
    /// Every folder, continuing the root pass's listing when it has one.
    Full(Option<Walk>),
}

/// The root pass's listing, applied onto the base.
pub struct Listed {
    pub(super) result: Result<(Vault, Walk)>,
    pub(super) read: Pending,
}

/// A full pass's walk, applied onto the base.
pub struct Scanned {
    pub(super) result: Result<Walked>,
}

pub(super) struct Walked {
    pub(super) vault: Vault,
    /// It differs from the base.
    pub(super) changed: bool,
    /// A scoped read failed: the next scan reads everything (watcher spec, F4).
    pub(super) incomplete: bool,
}

impl Scan {
    pub(super) fn new(base: Vault, read: Pending) -> Self {
        let pass = if base.is_scanned() {
            Pass::Full(None)
        } else {
            Pass::Root
        };
        Self { base, read, pass }
    }

    pub(super) fn full(base: Vault, read: Pending, seed: Option<Walk>) -> Self {
        Self {
            base,
            read,
            pass: Pass::Full(seed),
        }
    }

    /// The scan lists the root level before the rest (open-first spec, O6).
    #[must_use]
    pub fn is_initial(&self) -> bool {
        matches!(self.pass, Pass::Root)
    }

    /// Lists the root alone. The base drops here, off the foreground.
    pub async fn list_root(self, fs: &dyn Fs) -> Listed {
        let Self { base, read, .. } = self;
        let result = fs.read_dir(base.root()).await.map(|found| {
            let listing = Walk {
                read: vec![RelPath::root()],
                found,
                ..Walk::default()
            };
            (base.applied(listing.clone()), listing)
        });
        Listed { result, read }
    }

    /// Reads what the scan asks for and applies it onto the base, off the foreground (O1).
    pub async fn walk(self, fs: &dyn Fs, jobs: &dyn Jobs) -> Scanned {
        let Self { base, read, pass } = self;
        let root = base.root();
        let walk = match (pass, read) {
            (Pass::Full(Some(seed)), _) => Vault::walk_seeded(fs, root, seed, jobs).await,
            // A vault no walk has read whole is read whole, whatever the batch names (watcher spec, F4).
            (_, Pending::Folders(folders)) if base.has_read_whole() => {
                base.walk_changed(fs, &folders, jobs).await
            }
            _ => match Vault::walk(fs, root, &RelPath::root(), jobs).await {
                Ok(walk) => walk,
                Err(error) => return Scanned { result: Err(error) },
            },
        };
        let incomplete = walk.reach == Reach::Incomplete;
        let mut vault = base.applied(walk);
        vault.mark_scanned();
        let changed = vault != base;
        Scanned {
            result: Ok(Walked {
                vault,
                changed,
                incomplete,
            }),
        }
    }
}
