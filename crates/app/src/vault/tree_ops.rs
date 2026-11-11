use std::path::PathBuf;
use std::sync::Arc;

use futures::channel::oneshot;
use gpui::{App, Context, Task, prelude::*};
use inkra_vault::{EntryId, Fs, Kind, New};

use super::NoteStore;
use super::session::{Lifecycle, PendingSave, Writer};
use super::{NoteClosed, RenameFailed, TrashFailed};

/// What a create did. `Refused` made nothing on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Created {
    Listed(EntryId),
    /// The file exists; the vault does not list it yet.
    Unlisted,
    Refused,
}

impl NoteStore {
    /// Ordered on the note's chain: a write landing after the delete would
    /// recreate the file. A gone note closes, trashing only a file its own write put back (watcher spec, F9).
    pub(crate) fn trash(&mut self, id: EntryId, cx: &mut Context<Self>) -> Task<bool> {
        let has_entry = self
            .vault
            .as_ref()
            .is_some_and(|vault| vault.read(cx).entry(id).is_some());
        let Some(pending) = self.sessions.update(id, |life| life.begin_trash(has_entry)) else {
            return Task::ready(false);
        };

        let fs = Arc::clone(&self.fs);
        self.queue(id, pending, cx, async move |this, cx| {
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
                    this.sessions.update(id, Lifecycle::trash_failed);
                    cx.emit(TrashFailed(id));
                    return false;
                }
                // Closed only once the file is actually gone: an
                // optimistic close on a failed delete strands the buffer.
                drop(this.sessions.update(id, Lifecycle::trash_done));
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

    /// A None move means no path is safe; callers skip.
    pub(super) fn path_of(&self, id: EntryId, cx: &App) -> Option<PathBuf> {
        match self.moves.get(&id) {
            Some(moved) => moved.clone(),
            None => self.vault.as_ref()?.read(cx).absolute(id),
        }
    }

    /// The note's path under the vault root, following a rename ahead of its rescan; the session file's name for it.
    pub(crate) fn vault_path(&self, id: EntryId, cx: &App) -> Option<PathBuf> {
        let vault = self.vault.as_ref()?.read(cx);
        match self.moves.get(&id) {
            Some(Some(moved)) => moved.strip_prefix(vault.root()).ok().map(PathBuf::from),
            _ => Some(vault.entry(id)?.path.as_path().to_path_buf()),
        }
    }

    /// `None`: nothing on disk is the note's. With no entry, only the file at a gone note's last path
    /// may be: a trashing note claims nothing at a swap, so the vault may never see what the note's own save wrote.
    fn trash_target(&self, id: EntryId, cx: &App) -> Option<TrashTarget> {
        if let Some(path) = self.path_of(id, cx) {
            return Some(TrashTarget::Entry(path));
        }
        let (path, note) = self.sessions.get(id)?.life.last_write()?;
        Some(TrashTarget::LastWrite(path.clone(), note.writer()))
    }

    /// The id comes from `Fs::create`'s identity, never the path — a
    /// concurrent walk could resolve the path to a different file.
    pub(crate) fn create(
        &mut self,
        parent: Option<EntryId>,
        new: New,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Task<inkra_vault::Result<Created>> {
        let Some(vault) = self.vault.clone() else {
            return Task::ready(Ok(Created::Refused));
        };
        if inkra_vault::check_name(name).is_some() {
            return Task::ready(Ok(Created::Refused));
        }
        let file_name = Kind::from(new).file_name(name);

        let root = {
            let vault = vault.read(cx);
            match parent {
                None => vault.root().to_path_buf(),
                Some(parent) => match vault.entry(parent) {
                    Some(entry) if entry.kind == Kind::Dir => {
                        vault.absolute(parent).unwrap_or_default()
                    }
                    _ => return Task::ready(Ok(Created::Refused)),
                },
            }
        };
        let path = root.join(&file_name);
        let fs = Arc::clone(&self.fs);

        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move { fs.create(&path, new).await })
                .await?;

            let Ok(applied) = this.update(cx, Self::rescan) else {
                return Ok(Created::Unlisted);
            };
            if applied.await.is_err() {
                return Ok(Created::Unlisted);
            }

            Ok(this
                .update(cx, |this, cx| {
                    let id = this.vault.as_ref()?.read(cx).id_for(made);
                    cx.notify();
                    id
                })
                .ok()
                .flatten()
                .map_or(Created::Unlisted, Created::Listed))
        })
    }

    pub(crate) fn rename(&mut self, id: EntryId, name: &str, cx: &mut Context<Self>) -> Task<bool> {
        if inkra_vault::check_name(name).is_some() {
            return Task::ready(false);
        }
        if self.vault.is_none() {
            return Task::ready(false);
        }
        let name = name.to_owned();
        let fs = Arc::clone(&self.fs);
        let (rescanned, rescan_rx) = oneshot::channel();
        let queued = self.queue(id, PendingSave::Write, cx, async move |this, cx| {
            // Resolved here, not at issue time: an operation queued behind
            // a rename must act on the moved name.
            let Ok(Some((from, to, before))) = this.update(cx, |this, cx| {
                let vault = this.vault.as_ref()?.read(cx);
                let from = this.path_of(id, cx)?;
                let file_name = vault.entry(id)?.kind.file_name(&name);
                let to = from.parent()?.join(file_name);
                let before = this.moves.insert(id, None);
                Some((from, to, before))
            }) else {
                return false;
            };

            let moved = cx
                .background_spawn({
                    let to = to.clone();
                    async move { fs.rename(&from, &to).await }
                })
                .await;

            this.update(cx, |this, cx| {
                if let Err(error) = moved {
                    match before {
                        Some(prev) => this.moves.insert(id, prev),
                        None => this.moves.remove(&id),
                    };
                    cx.emit(RenameFailed { id, error });
                    return false;
                }
                this.moves.insert(id, Some(to.clone()));
                let _ = rescanned.send((to, this.rescan(cx)));
                true
            })
            .unwrap_or(false)
        });

        let (done, wait) = oneshot::channel();
        cx.spawn(async move |this, cx| {
            let moved = queued.await;
            let mut ok = moved;
            if moved && let Ok((to, applied)) = rescan_rx.await {
                let landed = applied.await.is_ok();
                let _ = this.update(cx, |this, cx| {
                    if landed && this.moves.get(&id) == Some(&Some(to)) {
                        this.moves.remove(&id);
                    }
                    cx.notify();
                });
            } else if moved {
                ok = false;
            }
            let _ = done.send(ok);
        })
        .detach();
        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
    }
}

enum TrashTarget {
    Entry(PathBuf),
    /// Trashed only if the file there has the stamp its `Writer` expects: the note's last write or load.
    /// No write lands between that read and the trash: saves run on the note's chain, the quit flush skips a trashing note.
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
