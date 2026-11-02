//! The write path: autosave, `⌘S` and the quit flush.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use gpui::{App, Context, Task, prelude::*};
use inkra_vault::EntryId;

use super::session::{Location, PendingSave, write_through};
use super::{ConflictPreserved, NoteSaved, NoteStore, SaveFailed};

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

    /// The quit's flush: on each note's chain, so a write behind a rename lands under the new name.
    pub(crate) fn flush(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let writes: Vec<_> = self
            .unsaved(cx)
            .into_iter()
            .map(|id| self.queue(id, PendingSave::Write, cx, async |_, _| true))
            .collect();
        cx.foreground_executor().spawn(async move {
            futures::future::join_all(writes).await;
        })
    }

    /// Copies buffers out synchronously: shutdown holds the `AppCell`
    /// borrowed, so a later borrow here would panic.
    pub(super) fn flush_all(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
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

        async move {
            futures::future::join_all(writes).await;
        }
    }

    fn write_path(&self, id: EntryId, location: &Location, cx: &App) -> Option<PathBuf> {
        match location {
            Location::Vault => self.vault.as_ref()?.read(cx).absolute(id),
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
                    cx.emit(SaveFailed(id));
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
            let Ok(saved) = written else {
                cx.emit(SaveFailed(id));
                return;
            };
            if let Some(note) = this.sessions.note_mut(id) {
                note.landed(version);
            }
            // An atomic save moves the inode: without the re-key the id orphans on the next rescan.
            this.reidentify(id, saved.identity, saved.stamp.mtime, cx);
            // The index's only signal for an ordinary write: it neither rescans nor notifies.
            cx.emit(NoteSaved {
                id,
                text,
                stamp: saved.stamp,
            });
            if let Some(preserved) = saved.preserved.clone() {
                cx.emit(ConflictPreserved {
                    id,
                    path: preserved,
                });
                drop(this.rescan(cx));
            }
            cx.notify();
        });
    }
}
