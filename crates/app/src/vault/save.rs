//! The write path: autosave, `⌘S` and the leave's flush.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use gpui::{App, Context, Task, prelude::*};
use inkra_vault::{EntryId, VaultError};

use super::session::{Location, OpenNote, PendingSave, write_through};
use super::{ConflictPreserved, NoteSaved, NoteStore, SaveFailed};

const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// Automatic retries after a note's first failure; an edit or `⌘S` still saves past it.
pub(super) const RETRY_LIMIT: u32 = 8;

/// The wait before retrying a note's `failures`th failed write: doubling from one second, capped.
pub(super) fn retry_delay(failures: u32) -> Duration {
    RETRY_FIRST
        .saturating_mul(1 << failures.saturating_sub(1).min(5))
        .min(RETRY_MAX)
}

/// An error a later attempt can clear by itself; a preserving failure is judged by its cause.
fn is_transient(error: &VaultError) -> bool {
    matches!(
        error.io_kind(),
        Some(
            ErrorKind::Interrupted
                | ErrorKind::WouldBlock
                | ErrorKind::TimedOut
                | ErrorKind::ResourceBusy
        )
    )
}

impl NoteStore {
    pub(super) fn schedule_save(&mut self, id: EntryId, delay: Duration, cx: &mut Context<Self>) {
        if self.sessions.note(id).is_none() {
            return;
        }
        let Some(cancelled) = self.sessions.arm(id, delay) else {
            return;
        };
        self.link(id, cx, async move |previous, this, cx| {
            let timer = cx.background_executor().timer(delay).fuse();
            if let Some(previous) = previous {
                previous.await;
            }
            futures::select_biased! {
                // A dropped sender also lands here; treating it as cancel
                // too is why `⌘S` then a keystroke can write nothing.
                cancelled = cancelled.fuse() => if cancelled.is_ok() { return },
                () = timer.fuse() => {}
            }
            // Before the write, not after, so a mid-write edit queues the
            // next write instead of being lost.
            let reopened = this.update(cx, |this, _| this.sessions.timer_passed(id));
            if reopened.is_err() {
                return;
            }
            Self::write(id, this, cx).await;
        });
    }

    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current {
            self.schedule_save(id, Duration::ZERO, cx);
        }
    }

    /// Open notes with unsaved text; a note mid-trash is not open, so it is never one.
    pub(crate) fn unsaved(&self, cx: &App) -> Vec<EntryId> {
        self.sessions
            .live()
            .filter(|(_, session)| session.life.is_open())
            .filter(|(_, session)| {
                session
                    .life
                    .bound_at()
                    .is_some_and(|(_, note)| note.is_dirty(cx))
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// The leave's flush: writes every unsaved note and joins every busy chain, so a queued rename lands.
    pub(crate) fn flush(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let unsaved = self.unsaved(cx);
        let busy: Vec<EntryId> = self
            .sessions
            .busy()
            .filter(|id| !unsaved.contains(id))
            .collect();
        let pending = unsaved
            .into_iter()
            .map(|id| (id, PendingSave::Write))
            .chain(busy.into_iter().map(|id| (id, PendingSave::Keep)));
        let joins: Vec<_> = pending
            .map(|(id, pending)| self.queue(id, pending, cx, async |_, _| true))
            .collect();
        cx.foreground_executor().spawn(async move {
            futures::future::join_all(joins).await;
        })
    }

    /// Copies buffers out synchronously: shutdown holds the `AppCell`
    /// borrowed, so a later borrow here would panic.
    pub(crate) fn flush_all(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = Vec<VaultError>> + use<> {
        let dirty: Vec<_> = self
            .sessions
            .live()
            // Skips a note mid-trash: this flush can't join its chain,
            // so writing here would recreate the file being deleted.
            .filter(|(_, session)| session.life.is_open())
            .filter_map(|(id, session)| {
                let (location, note) = session.life.bound_at()?;
                if !note.is_dirty(cx) {
                    return None;
                }
                Some((
                    note.writer(),
                    self.write_path(id, location, cx)?,
                    note.snapshot(cx).0,
                    location.folder(),
                ))
            })
            .collect();

        let fs = Arc::clone(&self.fs);
        let writes: Vec<_> = dirty
            .into_iter()
            .map(|(writer, path, text, folder)| {
                let seq = self.next_seq();
                cx.background_spawn(write_through(
                    writer,
                    Arc::clone(&fs),
                    path,
                    text,
                    seq,
                    folder,
                ))
            })
            .collect();

        // `None` is a write overtaken by a newer one, not a failure.
        async move {
            futures::future::join_all(writes)
                .await
                .into_iter()
                .flatten()
                .filter_map(Result::err)
                .collect()
        }
    }

    fn write_path(&self, id: EntryId, location: &Location, cx: &App) -> Option<PathBuf> {
        match location {
            Location::Vault => self.path_of(id, cx),
            Location::Detached(path) | Location::Gone(path) => Some(path.clone()),
        }
    }

    async fn write(id: EntryId, this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp) {
        let Ok(Some((fs, writer, path, text, version, seq, folder))) =
            this.update(cx, |this, cx| {
                let life = &this.sessions.get(id)?.life;
                let (location, note) = life.bound_at()?;
                let (text, version) = note.unsaved(cx)?;
                let Some(path) = this.write_path(id, location, cx) else {
                    this.failed(id, None, cx);
                    return None;
                };
                let (writer, folder) = (note.writer(), location.folder());
                let fs = Arc::clone(&this.fs);
                let seq = this.next_seq();
                Some((fs, writer, path, text, version, seq, folder))
            })
        else {
            return;
        };

        let written = cx
            .background_spawn(write_through(
                writer,
                fs,
                path,
                Arc::clone(&text),
                seq,
                folder,
            ))
            .await;
        // `None` means a newer write already landed; not a failure.
        let Some(written) = written else {
            return;
        };
        let _ = this.update(cx, |this, cx| {
            let saved = match written {
                Ok(saved) => saved,
                Err(error) => {
                    this.failed(id, Some(error), cx);
                    return;
                }
            };
            if let Some(note) = this.sessions.note_mut(id) {
                note.landed(version);
            }
            // An atomic save moves the inode: without the re-key the id orphans on the next rescan.
            this.reidentify(id, saved.identity, saved.stamp, cx);
            // The index's only signal for an ordinary write: it neither rescans nor notifies.
            cx.emit(NoteSaved {
                id,
                text,
                stamp: saved.stamp,
                identity: saved.identity,
            });
            if let Some(preserved) = saved.preserved {
                cx.emit(ConflictPreserved {
                    id,
                    path: preserved,
                });
                drop(this.rescan(cx));
            }
            cx.notify();
        });
    }

    /// Retries on the note's chain while the error is transient and under `RETRY_LIMIT`.
    fn failed(&mut self, id: EntryId, error: Option<VaultError>, cx: &mut Context<Self>) {
        let failure = SaveFailed { id, error };
        log::warn!(
            "save {:?}: {}",
            failure.id,
            failure
                .error
                .as_ref()
                .map_or("no path in the vault".into(), ToString::to_string)
        );
        if let Some(failures) = self.sessions.note_mut(failure.id).map(OpenNote::failed)
            && failures <= RETRY_LIMIT
            && failure.error.as_ref().is_some_and(is_transient)
        {
            self.schedule_save(failure.id, retry_delay(failures), cx);
        }
        let kept = match &failure.error {
            Some(VaultError::Unwritten { preserved, .. }) => Some(preserved.clone()),
            _ => None,
        };
        let id = failure.id;
        cx.emit(failure);
        // Last, so the conflict notice replaces the failure's and the retry's save does not clear it.
        if let Some(path) = kept {
            cx.emit(ConflictPreserved { id, path });
            drop(self.rescan(cx));
        }
        cx.notify();
    }
}
