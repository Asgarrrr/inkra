//! Loads and saves: one seq orders them per note, and only the latest acts (L1, L3, L5).

use std::path::PathBuf;

use inkra_vault::{EntryId, Mtime, Vault};

use super::LiveIndex;
use crate::content::NoteContent;

/// One load or save of one note; it acts only while it is the latest minted for that note.
#[derive(Clone, Copy, Debug)]
pub struct Ticket {
    id: EntryId,
    seq: u64,
    /// `note_sets` when it was minted.
    note_sets: u64,
}

/// A load to start: read `path`, parse it, hand the result to `LiveIndex::landed`.
#[derive(Debug)]
pub struct Load {
    pub ticket: Ticket,
    pub path: PathBuf,
}

/// A load started and not landed yet.
pub(super) struct Loading {
    pub(super) id: EntryId,
    seq: u64,
    /// False once its note left the vault: not pending, though it keeps its slot until it lands.
    pub(super) live: bool,
}

impl LiveIndex {
    /// A note saved: `mtime` is the one indexed, and the parse of its text applies through `landed`.
    #[must_use]
    pub(super) fn saved(&mut self, id: EntryId, mtime: Mtime) -> Ticket {
        let seq = self.next_seq(id);
        self.indexed.insert(id, mtime);
        self.ticket(id, seq)
    }

    /// The next queued note, while fewer than `slots` load; `vault` is the live one, to find its path.
    #[must_use]
    pub(super) fn next_load(&mut self, vault: &Vault, slots: usize) -> Option<Load> {
        if self.loading.len() >= slots {
            return None;
        }
        let mut busy = Vec::new();
        let next = loop {
            let Some((id, seq)) = self.queue.pop_front() else {
                break None;
            };
            if self.queued.get(&id) != Some(&seq) {
                continue;
            }
            // One read per note at a time, so a note rewritten in a loop cannot take every slot.
            if self.loading.iter().any(|load| load.id == id) {
                busy.push((id, seq));
                continue;
            }
            self.queued.remove(&id);
            if let Some(path) = vault.absolute(id) {
                break Some((id, seq, path));
            }
        };
        for entry in busy.into_iter().rev() {
            self.queue.push_front(entry);
        }
        let (id, seq, path) = next?;
        self.loading.push(Loading {
            id,
            seq,
            live: true,
        });
        Some(Load {
            ticket: self.ticket(id, seq),
            path,
        })
    }

    /// A load or a save's parse landed; true when the index changed. `vault` is the live one.
    pub(super) fn landed(
        &mut self,
        ticket: Ticket,
        loaded: inkra_vault::Result<NoteContent>,
        vault: &Vault,
    ) -> bool {
        let Ticket { id, seq, note_sets } = ticket;
        // First, so a retry can take the slot this load frees.
        if let Some(at) = self
            .loading
            .iter()
            .position(|load| load.id == id && load.seq == seq)
        {
            self.loading.swap_remove(at);
        }
        match loaded {
            Ok(content) => self.apply_content(id, seq, content),
            Err(_) => self.failed(id, seq, note_sets, vault),
        }
    }

    pub(super) fn queue_load(&mut self, id: EntryId) {
        let seq = self.next_seq(id);
        self.queue.push_back((id, seq));
        self.queued.insert(id, seq);
    }

    fn ticket(&self, id: EntryId, seq: u64) -> Ticket {
        Ticket {
            id,
            seq,
            note_sets: self.note_sets,
        }
    }

    fn next_seq(&mut self, id: EntryId) -> u64 {
        self.next_seq += 1;
        self.latest.insert(id, self.next_seq);
        self.next_seq
    }

    fn is_latest(&self, id: EntryId, seq: u64) -> bool {
        self.latest.get(&id) == Some(&seq)
    }

    fn apply_content(&mut self, id: EntryId, seq: u64, content: NoteContent) -> bool {
        if !self.is_latest(id, seq) {
            return false;
        }
        self.index.insert(id, content);
        self.revision += 1;
        self.forgotten.remove(&id);
        // Not a note of the index yet (its build is still running): the next build reloads it.
        if !self.index.is_indexed(id) {
            self.indexed.remove(&id);
        }
        true
    }

    /// A build since the load started may have moved the note; a rename keeps its mtime, so no rescan reloads it.
    fn failed(&mut self, id: EntryId, seq: u64, note_sets: u64, vault: &Vault) -> bool {
        let present = vault.entry(id).is_some();
        if note_sets != self.note_sets && present && self.is_latest(id, seq) {
            self.queue_load(id);
            false
        } else {
            // `forget` records the note in `forgotten`, so the next change of either kind reloads it.
            self.forget(id, seq)
        }
    }

    /// Drops links read before a failed load, unless a newer load or save is on its way.
    fn forget(&mut self, id: EntryId, seq: u64) -> bool {
        if !self.is_latest(id, seq) {
            return false;
        }
        self.indexed.remove(&id);
        self.forgotten.insert(id);
        self.index.remove(id);
        self.revision += 1;
        true
    }
}
