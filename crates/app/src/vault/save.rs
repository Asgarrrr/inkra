//! The write path: autosave, `⌘S` and the quit flush.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::channel::oneshot;
use gpui::{App, Context, prelude::*};
use inkra_vault::EntryId;

use super::store::{Location, OpenNote};
use super::writer::write_through;
use super::{ConflictPreserved, NoteSaved, NoteStore, SaveFailed};

impl NoteStore {
    pub(super) fn schedule_save(&mut self, id: EntryId, delay: Duration, cx: &mut Context<Self>) {
        if !self.open.contains_key(&id) {
            return;
        }
        let ops = self.ops.entry(id).or_default();
        if delay.is_zero() {
            ops.cancel_pending();
        } else if ops.scheduled {
            return;
        }

        let (cancel, cancelled) = oneshot::channel();
        ops.cancel = Some(cancel);
        ops.scheduled = !delay.is_zero();
        // Taken, not dropped: replacing the slot outright would cancel an
        // operation already queued behind it.
        let previous = ops.chain.take();

        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
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
            let reopened = this.update(cx, |this, _| {
                if let Some(ops) = this.ops.get_mut(&id) {
                    ops.scheduled = false;
                }
            });
            if reopened.is_err() {
                return;
            }
            Self::write(id, this, cx).await;
        }));
    }

    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.current {
            self.schedule_save(id, Duration::ZERO, cx);
        }
    }

    /// Copies buffers out synchronously: shutdown holds the `AppCell`
    /// borrowed, so a later borrow here would panic.
    pub(super) fn flush_all(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        let dirty: Vec<_> = self
            .open
            .iter()
            .filter_map(|(id, note)| {
                let buffer = note.buffer.read(cx);
                if buffer.version() == note.saved_version {
                    return None;
                }
                // Skips a note mid-trash: this flush can't join its chain,
                // so writing here would recreate the file being deleted.
                if self.ops.get(id).is_some_and(|ops| ops.trashing) {
                    return None;
                }
                Some((
                    Arc::clone(&note.writer),
                    self.write_path(*id, note, cx)?,
                    Arc::from(buffer.text()),
                    note.folder(),
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

    fn write_path(&self, id: EntryId, note: &OpenNote, cx: &App) -> Option<PathBuf> {
        match &note.location {
            Location::Vault => self.vault.as_ref()?.read(cx).absolute(id),
            Location::Detached(path) | Location::Gone(path) => Some(path.clone()),
        }
    }

    async fn write(id: EntryId, this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp) {
        let Ok(Some((fs, writer, path, text, version, seq, folder))) =
            this.update(cx, |this, cx| {
                let note = this.open.get(&id)?;
                // `update`, never `as_mut`: the latter notifies on drop even
                // though `finalize_transaction` changes no text.
                let (text, version) = note.buffer.update(cx, |buffer, _| {
                    buffer.finalize_transaction();
                    (Arc::<str>::from(buffer.text()), buffer.version())
                });
                if version == note.saved_version {
                    return None;
                }
                let Some(path) = this.write_path(id, note, cx) else {
                    cx.emit(SaveFailed(id));
                    return None;
                };
                let (writer, folder) = (Arc::clone(&note.writer), note.folder());
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
            if let Some(note) = this.open.get_mut(&id) {
                note.saved_version = version;
            }
            // An atomic save moves the inode: without the re-key the id orphans on the next rescan.
            this.reidentify(id, saved.identity, saved.stamp.mtime, cx);
            // The index's only signal for an ordinary write: it neither rescans nor notifies.
            if id != EntryId::DETACHED {
                cx.emit(NoteSaved {
                    id,
                    text,
                    stamp: saved.stamp,
                });
            }
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
