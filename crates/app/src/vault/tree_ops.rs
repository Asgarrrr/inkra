use std::sync::Arc;
use std::time::Duration;

use futures::channel::oneshot;
use gpui::{Context, Task, prelude::*};
use inkra_vault::{EntryId, Kind, Vault};

use super::NoteStore;
use super::{NoteClosed, RenameFailed, TrashFailed};

impl NoteStore {
    /// Moves one note to the platform trash, and closes it if it was open.
    ///
    /// Ordered on the note's chain rather than issued straight away, which is
    /// the whole reason the chain exists. A pending write that landed *after*
    /// the delete would recreate the file the user just removed; queued behind
    /// it instead, the write goes to disk first and the trash then takes the
    /// latest text with it — which is also the better thing to recover, since
    /// the Trash holds the only copy.
    ///
    /// Resolves to whether the note is gone. `false` covers a note with no
    /// vault entry, a note already being trashed, and a failed delete — the
    /// last of which also emits `TrashFailed`.
    pub(crate) fn trash(&mut self, id: EntryId, cx: &mut Context<Self>) -> Task<bool> {
        // A note named on the command line has no entry to remove and no row
        // to remove it from. Nothing in the UI can reach this — only the
        // sidebar issues the command, and its rows all come from the vault —
        // asserted rather than assumed, since `current` *can* be `DETACHED`
        // and a future caller might read it as the target.
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
        // Preempted rather than cancelled: a write still waiting out
        // `AUTOSAVE_DELAY` would otherwise hold the row on screen looking
        // broken, but cancelling it instead would trash the note with the
        // *previous* save's text — the only copy the user gets back.
        // `schedule_save` with no delay resolves the timer and leaves the
        // bookkeeping consistent; sending on `Ops::cancel` directly does not
        // — the chain returns before the line that clears `scheduled`, and
        // every later edit is absorbed by that flag.
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
            // Resolved here rather than at issue time, for the reason `rename`
            // gives: a delete queued behind a rename would otherwise carry the
            // path from before the move, find nothing there, and report a
            // failure for a note the user confirmed deleting.
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
                // Dropped only once the file is actually gone: an optimistic
                // close would leave the editor blank and the buffer
                // unreachable if the delete failed — worse than
                // `SaveFailed`, where the note at least stays visibly dirty.
                this.open.remove(&id);
                this.observers.remove(&id);
                if this.current == Some(id) {
                    this.current = None;
                }
                cx.emit(NoteClosed(id));
                // Dropped: this caller has no id to resolve afterwards. Only
                // `create` needs the walk to have landed.
                drop(this.rescan(cx));
                cx.notify();
                let _ = done.send(true);
            });
        }));

        cx.foreground_executor()
            .spawn(async move { wait.await == Ok(true) })
    }

    /// Makes an empty note or a folder, and resolves to the id the vault gives
    /// it.
    ///
    /// `parent` is a folder to put it in, or `None` for the vault root. A
    /// `parent` that names a note rather than a folder is the caller's mistake
    /// and resolves to `None`.
    ///
    /// On no chain, unlike `trash`: the file does not exist yet, so there is no
    /// note whose writes it could overtake. What it does have to order against
    /// is the vault listing, which is why it awaits its own rescan before
    /// answering — and why `rescan` hands back a receiver.
    ///
    /// The id comes from the identity `Fs::create` returned, never from the
    /// path: a walk landing between the create and the lookup would answer a
    /// path query with whatever holds that name *now*, and after an external
    /// move that is a different file.
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
        // The name rules live in `vault`, a property of a vault path.
        // Re-checked here rather than trusted: `create` is a public entry
        // point, and a future caller (a command palette, a missing-note
        // `[[link]]`) has no field to have validated for it.
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
            // An `Err` means the store was dropped mid-walk, which takes the
            // vault with it — there is no id left to report.
            applied.await.ok()?;

            this.update(cx, |this, cx| {
                let id = this.vault.as_ref()?.read(cx).id_for(made);
                cx.notify();
                id
            })
            .ok()?
        })
    }

    /// Renames one note or folder in place, keeping its `EntryId`.
    ///
    /// Ordered on the note's chain, like `trash` and for the same reason: a
    /// write decided before the rename must reach the old path first, or it
    /// lands afterwards and recreates the note under the name the user just
    /// moved away from.
    ///
    /// The rescan is awaited **inside** the chain link, before the next
    /// operation is let through: `Self::write` resolves its path through
    /// `Vault::absolute` at write time, so a write released while the listing
    /// still held the old path would write there — the recreation the
    /// ordering exists to prevent, arriving one link later.
    ///
    /// That await is **not under test**: reaching it needs a write issued
    /// between the rename landing and its rescan applying, and
    /// `run_until_parked` drains both. Same limit as the write-ordering tests
    /// — `TestScheduler` can model "queued and not yet polled" but not a
    /// background thread already inside the call.
    ///
    /// Resolves to whether the entry moved. `false` covers a missing entry, a
    /// refused name, and a failed rename, the last of which emits
    /// `RenameFailed`.
    pub(crate) fn rename(&mut self, id: EntryId, name: &str, cx: &mut Context<Self>) -> Task<bool> {
        // Same DETACHED guard as `trash`, for the same reason.
        if id == EntryId::DETACHED {
            return Task::ready(false);
        }
        if inkra_vault::check_name(name).is_some() {
            return Task::ready(false);
        }
        if self.vault.is_none() {
            return Task::ready(false);
        }
        // Preempted like `trash` (same reason: un-preempted, the row would
        // show its old name for up to `AUTOSAVE_DELAY`). See `trash` for why
        // preempt and not cancel.
        self.schedule_save(id, Duration::ZERO, cx);

        let name = name.to_owned();
        let fs = Arc::clone(&self.fs);
        let previous = self.ops.entry(id).or_default().chain.take();
        let (done, wait) = oneshot::channel();
        self.ops.entry(id).or_default().chain = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            // Resolved **here**, not at issue time, and for the reason
            // `Self::write` gives: an operation queued behind a rename would
            // otherwise carry the path from before it and act on a name that
            // has moved. Two renames of one note in a turn is the ordinary way
            // to reach it — the second would report `NotFound` — and a delete
            // behind a rename is the dangerous one, because a confirmed delete
            // would silently not happen.
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
            // Awaited here, inside the link: see the note above.
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

    /// Lists the vault again, so a file this process did not create through
    /// the editor reaches the tree.
    ///
    /// Not free, and not run on a timer: the conflict path and the three
    /// mutation commands are the only callers, on a user gesture.
    ///
    /// **Serialised** — load-bearing, not tidy. `Vault::apply` replaces
    /// `entries`/`by_identity` outright with no staleness check, so two walks
    /// in flight let whichever finishes last win: a walk taken before another
    /// mutation landed can re-admit a trashed note, or mint a fresh `EntryId`
    /// for a file whose buffer is tracked under the old one — clicking that
    /// row would then reload from disk and discard unsaved text. Chaining
    /// makes the last walk applied the last walk taken.
    ///
    /// Global, not per-note: the walk rebuilds the whole vault, so two in
    /// flight collide regardless of which notes caused them.
    ///
    /// Two halves because `Vault::apply` needs `&mut Vault` and the walk
    /// awaits; an entity hands out `&mut` only inside a synchronous closure.
    ///
    /// Returns a receiver, not the `Task`: the `Task` is the chain and stays
    /// here for the next walk to await. `create` is the one caller that needs
    /// the id its walk minted; every other caller drops the receiver.
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
            // `cx.spawn` is the *foreground* executor
            // (`gpui/src/app.rs:2036-2045`), and `RealFs::read_dir` blocks
            // inside its future by design — every other `Fs` call in this file
            // is wrapped the same way. A directory walk on the main thread
            // freezes the window, and a vault on a network mount is exactly
            // where both conflicts and slow walks come from.
            let walk = cx.background_spawn(async move { Vault::collect(&*fs, &root).await });
            let Ok(children) = walk.await else {
                return;
            };
            // The entity is held strongly by this closure, so neither this nor
            // the `read_with` above can fail on a released entity.
            vault.update(cx, |vault, cx| {
                vault.apply(children);
                cx.notify();
            });
            // After the apply, so a caller woken by this can read what the walk
            // found. An `Err` means the caller stopped caring and dropped it.
            let _ = applied.send(());
        }));
        wait
    }
}
