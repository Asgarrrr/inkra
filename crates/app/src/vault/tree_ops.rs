use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::channel::oneshot;
use gpui::{App, AsyncApp, Context, Task, WeakEntity, prelude::*};
use inkra_vault::{EntryId, Fs, Kind};

use super::NoteStore;
use super::store::Location;
use super::writer::Writer;
use super::{NoteClosed, RenameFailed, TrashFailed};

impl NoteStore {
    /// Ordered on the note's chain: a write landing after the delete would
    /// recreate the file. A gone note closes, trashing only a file its own write put back (watcher spec, F9).
    pub(crate) fn trash(&mut self, id: EntryId, cx: &mut Context<Self>) -> Task<bool> {
        if id == EntryId::DETACHED {
            return Task::ready(false);
        }
        let gone = self.is_gone(id);
        if !gone
            && self
                .vault
                .as_ref()
                .and_then(|v| v.read(cx).entry(id))
                .is_none()
        {
            return Task::ready(false);
        }
        let ops = self.ops.entry(id).or_default();
        if ops.trashing {
            return Task::ready(false);
        }
        ops.trashing = true;
        if gone {
            // Cancelled, not preempted: the write would recreate the file whose text the trash discards.
            ops.cancel_pending();
        } else {
            // Preempted, not cancelled: cancelling would trash the note with
            // the previous save's text, the only copy the user gets back.
            self.schedule_save(id, Duration::ZERO, cx);
        }

        let fs = Arc::clone(&self.fs);
        self.after_chain(id, cx, async move |this, cx| {
            // Resolved here, not at issue time: a delete queued behind a
            // rename must use the path after the move.
            let Ok(target) = this.update(cx, |this, cx| this.trash_target(id, cx)) else {
                return false;
            };
            let removed = match target {
                Some(target) => cx
                    .background_spawn(async move { target.trash(&*fs).await })
                    .await
                    .is_ok(),
                None => true,
            };

            this.update(cx, |this, cx| {
                if !removed {
                    if let Some(ops) = this.ops.get_mut(&id) {
                        ops.trashing = false;
                    }
                    cx.emit(TrashFailed(id));
                    return false;
                }
                // Closed only once the file is actually gone: an
                // optimistic close on a failed delete strands the buffer.
                this.open.remove(&id);
                this.observers.remove(&id);
                if this.current == Some(id) {
                    this.current = None;
                }
                cx.emit(NoteClosed(id));
                drop(this.rescan(cx));
                cx.notify();
                true
            })
            .unwrap_or(false)
        })
    }

    /// `None`: nothing on disk is the note's. With no entry, only the file at a gone note's last path
    /// may be: `trashing` stops its reclaim, so the vault may never see what the note's own save wrote.
    fn trash_target(&self, id: EntryId, cx: &App) -> Option<TrashTarget> {
        if let Some(path) = self.vault.as_ref()?.read(cx).absolute(id) {
            return Some(TrashTarget::Entry(path));
        }
        let note = self.open.get(&id)?;
        match &note.location {
            Location::Gone(path) => Some(TrashTarget::LastWrite(path.clone(), note.writer())),
            Location::Vault | Location::Detached(_) => None,
        }
    }

    /// The id comes from `Fs::create`'s identity, never the path — a
    /// concurrent walk could resolve the path to a different file.
    pub(crate) fn create(
        &mut self,
        parent: Option<EntryId>,
        kind: Kind,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Task<Option<EntryId>> {
        let Some(vault) = self.vault.clone() else {
            return Task::ready(None);
        };
        if inkra_vault::check_name(name).is_some() {
            return Task::ready(None);
        }
        let file_name = match kind {
            Kind::Note => inkra_vault::note_file_name(name),
            Kind::Dir => name.trim().to_owned(),
        };

        let root = {
            let vault = vault.read(cx);
            match parent {
                None => vault.root().to_path_buf(),
                Some(parent) => match vault.entry(parent) {
                    Some(entry) if entry.kind == Kind::Dir => {
                        vault.absolute(parent).unwrap_or_default()
                    }
                    _ => return Task::ready(None),
                },
            }
        };
        let path = root.join(&file_name);
        let fs = Arc::clone(&self.fs);

        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move { fs.create(&path, kind).await })
                .await
                .ok()?;

            let applied = this.update(cx, Self::rescan).ok()?;
            applied.await.ok()?;

            this.update(cx, |this, cx| {
                let id = this.vault.as_ref()?.read(cx).id_for(made);
                cx.notify();
                id
            })
            .ok()?
        })
    }

    pub(crate) fn rename(&mut self, id: EntryId, name: &str, cx: &mut Context<Self>) -> Task<bool> {
        if id == EntryId::DETACHED {
            return Task::ready(false);
        }
        if inkra_vault::check_name(name).is_some() {
            return Task::ready(false);
        }
        if self.vault.is_none() {
            return Task::ready(false);
        }
        self.schedule_save(id, Duration::ZERO, cx);

        let name = name.to_owned();
        let fs = Arc::clone(&self.fs);
        self.after_chain(id, cx, async move |this, cx| {
            // Resolved here, not at issue time: an operation queued behind
            // a rename must act on the moved name.
            let Ok(Some((from, to))) = this.update(cx, |this, cx| {
                let vault = this.vault.as_ref()?.read(cx);
                let from = vault.absolute(id)?;
                let file_name = match vault.entry(id)?.kind {
                    Kind::Note => inkra_vault::note_file_name(&name),
                    Kind::Dir => name.trim().to_owned(),
                };
                let to = from.parent()?.join(&file_name);
                Some((from, to))
            }) else {
                return false;
            };

            let moved = cx
                .background_spawn(async move { fs.rename(&from, &to).await })
                .await;

            if moved.is_err() {
                let _ = this.update(cx, |_, cx| cx.emit(RenameFailed(id)));
                return false;
            }
            // Awaited inside the chain link: a write released before this
            // lands would resolve the old path and recreate the note there.
            let Ok(applied) = this.update(cx, Self::rescan) else {
                return false;
            };
            let _ = applied.await;
            let _ = this.update(cx, |_, cx| cx.notify());
            true
        })
    }

    /// Runs `op` after every operation already queued on the note's chain,
    /// and resolves to its result.
    pub(super) fn after_chain<Op>(
        &mut self,
        id: EntryId,
        cx: &mut Context<Self>,
        op: Op,
    ) -> Task<bool>
    where
        Op: AsyncFnOnce(WeakEntity<Self>, &mut AsyncApp) -> bool + 'static,
    {
        let ops = self.ops.entry(id).or_default();
        let previous = ops.chain.take();
        let (done, wait) = oneshot::channel();
        ops.chain = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let _ = done.send(op(this, cx).await);
        }));
        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
    }
}

enum TrashTarget {
    Entry(PathBuf),
    /// Trashed only if the file there has the stamp its `Writer` expects: the note's last write or load.
    LastWrite(PathBuf, Arc<Writer>),
}

impl TrashTarget {
    async fn trash(self, fs: &dyn Fs) -> inkra_vault::Result<()> {
        match self {
            Self::Entry(path) => fs.trash(&path).await,
            Self::LastWrite(path, writer) => {
                let expected = writer.expected().await;
                // A load, not a stat seam: trashing a gone note is rare and a note is small.
                match fs.load(&path).await {
                    Ok(loaded) if loaded.stamp == expected => fs.trash(&path).await,
                    _ => Ok(()),
                }
            }
        }
    }
}
