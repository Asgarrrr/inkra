//! A clean open note follows the disk (watcher spec, F7 and F8).

use std::sync::Arc;

use gpui::{AsyncApp, Context, Entity, WeakEntity, prelude::*};
use inkra_vault::{EntryId, Vault};

use super::session::PendingSave;
use super::{GoneChanged, NoteStore};

impl NoteStore {
    /// After a swap, one pass over the open notes: gone from disk, back, or changed (F7, F9).
    pub(super) fn check_open_notes(
        &mut self,
        vault: &Entity<Vault>,
        previous: &Vault,
        cx: &mut Context<Self>,
    ) {
        let live = vault.read(cx);
        let swaps = self.sessions.swapped(
            |id| {
                let before = previous.entry(id).map(|entry| entry.mtime);
                (live.entry(id).map(|entry| entry.mtime), before)
            },
            // A rename's `moves` entry is ahead of `previous` until its rescan lands.
            |id| match self.moves.get(&id) {
                Some(moved) => moved.clone(),
                None => previous.absolute(id),
            },
        );
        for (id, _) in swaps.iter().filter(|(_, swap)| swap.flipped) {
            cx.emit(GoneChanged(*id));
        }
        for id in swaps
            .iter()
            .filter(|(_, swap)| swap.reload)
            .map(|(id, _)| *id)
        {
            // Behind every operation queued on the note, so it sees their stamp and `saved_version`.
            drop(
                self.queue(id, PendingSave::Keep, cx, async move |this, cx| {
                    Self::reload(id, this, cx).await.is_some()
                }),
            );
        }
    }

    /// Every foreground step looks the note up afresh and stops if it closed.
    async fn reload(id: EntryId, this: WeakEntity<Self>, cx: &mut AsyncApp) -> Option<()> {
        let (fs, writer, path, mtime) = this
            .update(cx, |this, cx| {
                let life = &this.sessions.get(id)?.life;
                let note = life.bound()?;
                if !life.is_open() || note.is_dirty(cx) {
                    return None;
                }
                let vault = this.vault.as_ref()?.read(cx);
                let path = vault.absolute(id)?;
                let mtime = vault.entry(id)?.mtime;
                Some((Arc::clone(&this.fs), note.writer(), path, mtime))
            })
            .ok()??;

        let loaded = cx
            .background_spawn(async move {
                let expected = writer.expected().await;
                // Our own save, whose re-key the swap has not seen yet.
                if mtime == expected.mtime {
                    return None;
                }
                let loaded = fs
                    .load(&path)
                    .await
                    .inspect_err(|error| log::warn!("vault reload: {error}"))
                    .ok()?;
                (loaded.stamp != expected).then_some(loaded)
            })
            .await?;

        let (text, version) = this
            .update(cx, |this, cx| {
                let note = this.sessions.note(id)?;
                if note.is_dirty(cx) {
                    return None;
                }
                Some(note.snapshot(cx))
            })
            .ok()??;

        let stamp = loaded.stamp;
        let diff = cx
            .background_spawn(
                async move { Arc::new(inkra_text::diff(version, &text, &loaded.text)) },
            )
            .await;

        #[cfg(test)]
        if let Some(held) = this.update(cx, |this, _| this.held_reload.take()).ok()? {
            let _ = held.await;
        }

        let adopt = this
            .update(cx, |this, cx| {
                this.sessions.note_mut(id)?.did_reload(&diff, stamp, cx)
            })
            .ok()??;
        cx.background_spawn(adopt.run()).await;
        Some(())
    }

    /// The sender releases the next reload, parked between its diff and its apply.
    #[cfg(test)]
    pub(crate) fn hold_reload(&mut self) -> futures::channel::oneshot::Sender<()> {
        let (release, held) = futures::channel::oneshot::channel();
        self.held_reload = Some(held);
        release
    }
}
