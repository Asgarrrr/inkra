use std::sync::Arc;
use std::time::Duration;

use futures::channel::oneshot;
use gpui::{Context, Task, prelude::*};
use inkra_vault::{EntryId, Kind, Vault};

use super::NoteStore;
use super::{NoteClosed, RenameFailed, TrashFailed};

impl NoteStore {
    /// Ordered on the note's chain: a write landing after the delete would
    /// recreate the file.
    pub(crate) fn trash(&mut self, id: EntryId, cx: &mut Context<Self>) -> Task<bool> {
        if id == EntryId::DETACHED {
            return Task::ready(false);
        }
        if self
            .vault
            .as_ref()
            .and_then(|v| v.read(cx).entry(id))
            .is_none()
        {
            return Task::ready(false);
        }
        if self.ops.get(&id).is_some_and(|ops| ops.trashing) {
            return Task::ready(false);
        }
        // Preempted, not cancelled: cancelling would trash the note with
        // the previous save's text, the only copy the user gets back.
        self.schedule_save(id, Duration::ZERO, cx);

        let ops = self.ops.entry(id).or_default();
        ops.trashing = true;
        let previous = ops.chain.take();

        let fs = Arc::clone(&self.fs);
        let (done, wait) = oneshot::channel();
        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            // Resolved here, not at issue time: a delete queued behind a
            // rename must use the path after the move.
            let Ok(Some(path)) =
                this.update(cx, |this, cx| this.vault.as_ref()?.read(cx).absolute(id))
            else {
                if let Ok(()) = this.update(cx, |this, cx| {
                    if let Some(ops) = this.ops.get_mut(&id) {
                        ops.trashing = false;
                    }
                    cx.emit(TrashFailed(id));
                }) {
                    let _ = done.send(false);
                }
                return;
            };
            let removed = cx
                .background_spawn(async move { fs.trash(&path).await })
                .await;

            let _ = this.update(cx, |this, cx| {
                if removed.is_err() {
                    if let Some(ops) = this.ops.get_mut(&id) {
                        ops.trashing = false;
                    }
                    cx.emit(TrashFailed(id));
                    let _ = done.send(false);
                    return;
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
                let _ = done.send(true);
            });
        }));

        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
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
        let previous = self.ops.entry(id).or_default().chain.take();
        let (done, wait) = oneshot::channel();
        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
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
                let _ = done.send(false);
                return;
            };

            let moved = cx
                .background_spawn(async move { fs.rename(&from, &to).await })
                .await;

            if moved.is_err() {
                let _ = this.update(cx, |_, cx| cx.emit(RenameFailed(id)));
                let _ = done.send(false);
                return;
            }
            // Awaited inside the chain link: a write released before this
            // lands would resolve the old path and recreate the note there.
            let Ok(applied) = this.update(cx, Self::rescan) else {
                return;
            };
            let _ = applied.await;
            let _ = this.update(cx, |_, cx| cx.notify());
            let _ = done.send(true);
        }));

        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
    }

    /// `Vault::apply` replaces its maps outright with no staleness check:
    /// concurrent walks can re-admit a trashed note or orphan a buffer's `EntryId`.
    pub(super) fn rescan(&mut self, cx: &mut Context<Self>) -> oneshot::Receiver<()> {
        let (applied, wait) = oneshot::channel();
        let Some(vault) = self.vault.clone() else {
            return wait;
        };
        let fs = Arc::clone(&self.fs);
        let previous = self.rescan.take();
        self.rescan = Some(cx.spawn(async move |_, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let root = vault.read_with(cx, |vault, _| vault.root().to_path_buf());
            // `RealFs::read_dir` blocks inside its future by design, so run
            // it on the background executor or it freezes the window.
            let walk = cx.background_spawn(async move { Vault::collect(&*fs, &root).await });
            let Ok(children) = walk.await else {
                return;
            };
            vault.update(cx, |vault, cx| {
                vault.apply(children);
                cx.notify();
            });
            let _ = applied.send(());
        }));
        wait
    }
}
