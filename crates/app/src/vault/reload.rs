//! A clean open note follows the disk (watcher spec, F7 and F8).

use std::sync::Arc;

use gpui::{AsyncApp, Context, Entity, WeakEntity, prelude::*};
use inkra_vault::{EntryId, Vault};

use super::store::Location;
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
        let mut changed = Vec::new();
        let mut flipped = Vec::new();
        for (&id, note) in &mut self.open {
            if id == EntryId::DETACHED {
                continue;
            }
            let Some(entry) = live.entry(id) else {
                // Our own trash closes the note; it is never gone.
                if note.is_gone() || self.ops.get(&id).is_some_and(|ops| ops.trashing) {
                    continue;
                }
                if let Some(path) = previous.absolute(id) {
                    note.location = Location::Gone(path);
                    flipped.push(id);
                }
                continue;
            };
            if note.is_gone() {
                note.location = Location::Vault;
                flipped.push(id);
            }
            if previous
                .entry(id)
                .is_none_or(|before| before.mtime != entry.mtime)
            {
                changed.push(id);
            }
        }
        for id in flipped {
            cx.emit(GoneChanged(id));
        }
        for id in changed {
            // Behind every operation queued on the note, so it sees their stamp and `saved_version`.
            drop(self.after_chain(id, cx, async move |this, cx| {
                Self::reload(id, this, cx).await.is_some()
            }));
        }
    }

    /// Every foreground step looks the note up afresh and stops if it left `open`.
    async fn reload(id: EntryId, this: WeakEntity<Self>, cx: &mut AsyncApp) -> Option<()> {
        let (fs, writer, path, mtime) = this
            .update(cx, |this, cx| {
                let note = this.open.get(&id)?;
                if this.ops.get(&id).is_some_and(|ops| ops.trashing) || this.is_dirty(id, cx) {
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
                    .inspect_err(|error| eprintln!("vault reload: {error}"))
                    .ok()?;
                (loaded.stamp != expected).then_some(loaded)
            })
            .await?;

        let (text, version) = this
            .update(cx, |this, cx| {
                let note = this.open.get(&id)?;
                if this.is_dirty(id, cx) {
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

        let writer = this
            .update(cx, |this, cx| this.open.get_mut(&id)?.did_reload(&diff, cx))
            .ok()??;

        // After the apply: adopted before a refused one, the next save would drop theirs unpreserved.
        cx.background_spawn(async move { writer.adopt(stamp).await })
            .await;
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
