//! Vault search over texts read now: the candidates' reads, in vault order, against the query.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::{EntryId, Stamp};

use super::hit::{NoteHits, Searched};
use super::run::{HITS_PER_NOTE, note_hits};
use crate::find::Terms;
use crate::words::NoteQuery;

/// After this many failed reads a search stops: the disk is failing, not the notes.
pub const READ_FAILURES: usize = 8;

/// A note whose words hold every term: read it, then hand the read to `TextSearch::read`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: EntryId,
    /// Relative to the vault root, as scanned.
    pub path: Arc<str>,
    /// The stamp of the text its words came from.
    pub stamp: Stamp,
}

/// What reading a candidate gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    Text {
        text: Arc<str>,
        stamp: Stamp,
    },
    Gone,
    NotUtf8,
    /// Any other error: the note may be fine, so it is not stale.
    Failed,
}

/// A search's answer, and the notes its reads found changed or gone since their load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFound {
    pub searched: Searched,
    /// In read order; empty when the search stopped on failures.
    pub stale: Vec<EntryId>,
    pub reads: usize,
}

/// One vault search fed by reads in candidate order.
pub struct TextSearch<'q> {
    terms: Terms<'q>,
    limit: usize,
    notes: Vec<NoteHits>,
    count: usize,
    stale: Vec<EntryId>,
    reads: usize,
    failures: usize,
}

impl<'q> TextSearch<'q> {
    #[must_use]
    pub fn new(query: &'q NoteQuery, limit: usize) -> Self {
        Self {
            terms: Terms::new(query),
            limit,
            notes: Vec::new(),
            count: 0,
            stale: Vec::new(),
            reads: 0,
            failures: 0,
        }
    }

    /// Searches `candidate`'s `read`; false once the search wants no more reads.
    pub fn read(&mut self, candidate: &Candidate, read: Read, cancel: &AtomicBool) -> bool {
        self.reads += 1;
        match read {
            Read::Text { text, stamp } => {
                if stamp != candidate.stamp {
                    self.stale.push(candidate.id);
                }
                let room = HITS_PER_NOTE.min(self.limit - self.count);
                let Some(group) = note_hits(candidate.id, &text, &self.terms, room, cancel) else {
                    return false;
                };
                // A text rewritten since its load may no longer hold the terms.
                if !group.hits.is_empty() {
                    self.count += group.hits.len();
                    self.notes.push(group);
                }
            }
            Read::Gone | Read::NotUtf8 => self.stale.push(candidate.id),
            Read::Failed => self.failures += 1,
        }
        self.failures < READ_FAILURES && self.count < self.limit
    }

    /// The hits found so far.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.count
    }

    /// The answer; `None` once `cancel` is set.
    #[must_use]
    pub fn finish(self, cancel: &AtomicBool) -> Option<TextFound> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let stopped = self.failures >= READ_FAILURES;
        Some(TextFound {
            searched: Searched {
                notes: self.notes,
                limit_reached: self.count >= self.limit,
            },
            stale: if stopped { Vec::new() } else { self.stale },
            reads: self.reads,
        })
    }
}
