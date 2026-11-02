//! The scan worker's rules: serial scans, coalesced requests, re-keys replayed at the swap (open-first spec, O2).

use futures::channel::oneshot;

use crate::fs::{FileIdentity, Mtime, Result, VaultError};
use crate::vault::{EntryId, Vault};
use crate::watch::Pending;

mod claims;
mod run;

pub use claims::Claims;
pub use run::{Listed, Scan, Scanned};

/// One scan at a time; `Default` is idle.
#[derive(Default)]
pub struct Scans {
    /// Between a scan's start and its full swap.
    running: Option<Session>,
}

#[derive(Default)]
struct Session {
    /// Answered once the running scan swaps in.
    answers: Vec<oneshot::Sender<()>>,
    /// The scan after this one, which starts once this one has swapped.
    next: Next,
    /// Re-keys since the running scan took its base, replayed at its swap.
    rekeys: Vec<Rekey>,
}

/// What a scan reads and who waits for it: only a request waits, and a request reads everything.
enum Next {
    /// A watch's batches, merged (watcher spec, F4).
    Batch(Pending),
    /// `rescan()` requests, answered once the scan swaps in.
    Rescan(Vec<oneshot::Sender<()>>),
}

impl Default for Next {
    fn default() -> Self {
        Self::Batch(Pending::default())
    }
}

/// A save's re-key, recorded during a scan.
struct Rekey {
    id: EntryId,
    identity: FileIdentity,
    mtime: Mtime,
    /// The live vault already mapped `identity` to `id`: the save kept the inode (watcher spec, F5).
    in_place: bool,
}

/// What a swap did to the live vault.
pub struct Swapped {
    /// The live vault changed: the caller notifies its observers.
    pub notify: bool,
    /// The state the result replaced, for the open-note check; the walk's error if it failed.
    pub replaced: Result<Vault>,
}

impl Scans {
    /// A rescan request: resolves once a scan that started after this call has swapped in, and
    /// is dropped if that scan's walk fails. The scan to run, if none was running.
    pub fn request(&mut self, live: &Vault) -> (oneshot::Receiver<()>, Option<Scan>) {
        let (answer, wait) = oneshot::channel();
        let scan = match &mut self.running {
            Some(Session {
                next: Next::Rescan(waiting),
                ..
            }) => {
                waiting.push(answer);
                None
            }
            Some(session) => {
                session.next = Next::Rescan(vec![answer]);
                None
            }
            None => Some(self.begin(Next::Rescan(vec![answer]), live)),
        };
        (wait, scan)
    }

    /// A watch's batch: read now, or merged into what the scan after the running one reads.
    pub fn batch(&mut self, batch: Pending, live: &Vault) -> Option<Scan> {
        if batch.is_empty() {
            return None;
        }
        match &mut self.running {
            Some(Session {
                next: Next::Batch(pending),
                ..
            }) => pending.merge(batch),
            // A request reads everything already.
            Some(_) => {}
            None => return Some(self.begin(Next::Batch(batch), live)),
        }
        None
    }

    /// After every save: applied to `live` now, or recorded while a scan runs. Whether `live` changed.
    pub fn reidentify(
        &mut self,
        live: &mut Vault,
        id: EntryId,
        identity: FileIdentity,
        mtime: Mtime,
    ) -> bool {
        let Some(session) = &mut self.running else {
            return live.reidentify(id, identity, mtime);
        };
        session.rekeys.push(Rekey {
            id,
            identity,
            mtime,
            in_place: live.id_for(identity) == Some(id),
        });
        false
    }

    /// Swaps the root pass in and returns the full pass, based on the result, in the same call.
    pub fn swap_root(
        &mut self,
        live: &mut Vault,
        listed: Listed,
        claims: &Claims,
    ) -> (Swapped, Scan) {
        let rekeys = self
            .running
            .as_mut()
            .map(|session| std::mem::take(&mut session.rekeys))
            .unwrap_or_default();
        let (swapped, seed) = match listed.result {
            Ok((next, listing)) => (replace(live, next, true, rekeys, claims), Some(listing)),
            Err(error) => (fail(live, rekeys, error), None),
        };
        // Taken in the call that swaps: no re-key falls between the root pass and the full pass's base.
        (swapped, Scan::full(live.clone(), listed.read, seed))
    }

    /// Swaps a full pass in, ends its scan, and returns the next one.
    pub fn swap(
        &mut self,
        live: &mut Vault,
        scanned: Scanned,
        claims: &Claims,
    ) -> (Swapped, Option<Scan>) {
        let Session {
            answers,
            next,
            rekeys,
        } = self.running.take().unwrap_or_default();
        let (swapped, incomplete) = match scanned.result {
            Ok(walked) => (
                replace(live, walked.vault, walked.changed, rekeys, claims),
                walked.incomplete,
            ),
            Err(error) => {
                let mut swapped = fail(live, rekeys, error);
                // O8: the index builds on what is known, so quick open never waits forever.
                swapped.notify |= !live.is_scanned();
                live.mark_scanned();
                (swapped, false)
            }
        };
        if swapped.replaced.is_ok() {
            for answer in answers {
                let _ = answer.send(());
            }
        }
        let next = match next {
            Next::Batch(_) if incomplete => Next::Batch(Pending::Everything),
            Next::Batch(pending) if pending.is_empty() => return (swapped, None),
            next => next,
        };
        (swapped, Some(self.begin(next, live)))
    }

    fn begin(&mut self, next: Next, live: &Vault) -> Scan {
        let (answers, read) = match next {
            Next::Batch(read) => (Vec::new(), read),
            Next::Rescan(waiting) => (waiting, Pending::Everything),
        };
        self.running = Some(Session {
            answers,
            ..Session::default()
        });
        // Taken in the call that starts recording: every re-key from here on is replayed.
        Scan::new(live.clone(), read)
    }
}

/// Reclaim, replay, replace: reclaiming first lets a replayed save to a gone note find its id (watcher spec, F9).
fn replace(
    live: &mut Vault,
    mut next: Vault,
    changed: bool,
    rekeys: Vec<Rekey>,
    claims: &Claims,
) -> Swapped {
    let mut notify = claims.reclaim(&mut next, live) || changed;
    for rekey in rekeys {
        notify |= next.replay_reidentify(rekey.id, rekey.identity, rekey.mtime, rekey.in_place);
    }
    Swapped {
        notify,
        replaced: Ok(std::mem::replace(live, next)),
    }
}

/// The walk failed: its re-keys apply to the live vault, which its base, dropped with the walk, no longer shares.
fn fail(live: &mut Vault, rekeys: Vec<Rekey>, error: VaultError) -> Swapped {
    let mut notify = false;
    for rekey in rekeys {
        notify |= live.reidentify(rekey.id, rekey.identity, rekey.mtime);
    }
    Swapped {
        notify,
        replaced: Err(error),
    }
}

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod swap_tests;

#[cfg(test)]
mod history_tests;
