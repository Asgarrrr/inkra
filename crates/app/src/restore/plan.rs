//! A read session mapped onto the vault walked whole, then onto the notes that loaded (restore-tabs spec, R6).

use std::collections::HashMap;
use std::ffi::OsStr;

use inkra_vault::{EntryId, Kind, Vault};

use super::{SavedTab, Session};
use crate::editor::ViewState;

/// A saved tab whose path names a note.
struct Survivor {
    saved_ix: usize,
    note: EntryId,
    view: Option<ViewState>,
}

/// The saved tabs whose path names a note, and the saved active index.
pub(crate) struct Plan {
    survivors: Vec<Survivor>,
    saved_active: usize,
}

/// A tab whose note loaded, with its buffer.
pub(crate) struct Arrival<B> {
    pub(crate) note: EntryId,
    pub(crate) buffer: B,
    pub(crate) view: Option<ViewState>,
}

/// The tabs whose note loaded: the one to show, the rest in bar order, and where the shown one stands.
pub(crate) struct Restored<B> {
    pub(crate) shown: Arrival<B>,
    pub(crate) parked: Vec<Arrival<B>>,
    pub(crate) active: usize,
}

fn path(tab: &SavedTab) -> Option<&OsStr> {
    tab.note.as_deref().map(OsStr::new)
}

impl Plan {
    /// Byte-exact on each path, in one pass over the entries; `None` if no tab names a note.
    pub(crate) fn resolve(session: &Session, vault: &Vault) -> Option<Self> {
        let mut found: HashMap<&OsStr, Option<EntryId>> = session
            .tabs
            .iter()
            .filter_map(path)
            .map(|path| (path, None))
            .collect();
        for entry in vault.entries().filter(|entry| entry.kind == Kind::Note) {
            if let Some(id) = found.get_mut(entry.path.as_path().as_os_str()) {
                *id = Some(entry.id);
            }
        }
        let survivors: Vec<_> = session
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(saved_ix, tab)| {
                Some(Survivor {
                    saved_ix,
                    note: (*found.get(path(tab)?)?)?,
                    view: tab.view.map(ViewState::from),
                })
            })
            .collect();
        (!survivors.is_empty()).then_some(Self {
            survivors,
            saved_active: session.active,
        })
    }

    /// The loads to join, in bar order.
    pub(crate) fn notes(&self) -> impl Iterator<Item = EntryId> + '_ {
        self.survivors.iter().map(|survivor| survivor.note)
    }

    /// Keeps the tabs whose load gave a buffer; `buffers` follows `notes`. `None` if none did.
    pub(crate) fn restored<B>(self, buffers: Vec<Option<B>>) -> Option<Restored<B>> {
        let (mut arrivals, saved_ixs): (Vec<_>, Vec<_>) = self
            .survivors
            .into_iter()
            .zip(buffers)
            .filter_map(|(survivor, buffer)| {
                let arrival = Arrival {
                    note: survivor.note,
                    buffer: buffer?,
                    view: survivor.view,
                };
                Some((arrival, survivor.saved_ix))
            })
            .unzip();
        // The saved active tab, else its nearest survivor to the left, else the first.
        let active = saved_ixs
            .iter()
            .rposition(|&saved_ix| saved_ix <= self.saved_active)
            .unwrap_or(0);
        (!arrivals.is_empty()).then(|| Restored {
            shown: arrivals.remove(active),
            parked: arrivals,
            active,
        })
    }
}
