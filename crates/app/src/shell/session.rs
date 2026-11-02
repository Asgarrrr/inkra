//! The session file behind the shell: throttled writes, the quit write, and the one-shot restore (restore-tabs spec, R5–R6).

use std::path::PathBuf;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Subscription, Task, Window};
use inkra_tabs::Place as _;
use inkra_tabs::Tabs;
use inkra_text::Buffer;

use super::parked::Parked;
use super::{AppShell, Shown};
use crate::editor::ParkedView;
use crate::restore::{self, Plan, Restored, Session, SessionWriter};
use crate::vault::NoteStore;

/// A crash loses at most this much of the tab changes, never text: autosave writes that.
pub(super) const THROTTLE: Duration = Duration::from_secs(1);

/// Only `Done` writes: an earlier write would replace the saved tabs with the launch's one tab.
pub(super) enum Restore {
    // Named: a tuple field held only for its `Drop` fails `dead_code`.
    Loading {
        _read: Task<()>,
    },
    Waiting(Session),
    /// Held outside `activation`: a user open replaces that, and must not cancel the restore.
    Landing {
        _loads: Task<()>,
    },
    Done,
}

/// The session file's writer, its throttle and its quit write; a shell without one writes nothing.
pub(super) struct SessionFile {
    writer: SessionWriter,
    /// One write per `THROTTLE` at most: a change arms it only if it is not armed.
    throttle: Option<Task<()>>,
    _quit: Subscription,
}

impl AppShell {
    /// A vault launch with a session file starts reading it; any other launch is `Done` with no file.
    pub(super) fn open_session(
        file: Option<PathBuf>,
        notes: &Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> (Option<SessionFile>, Restore) {
        let (fs, root) = {
            let notes = notes.read(cx);
            let root = notes
                .vault()
                .map(|vault| vault.read(cx).root().to_path_buf());
            (notes.fs(), root)
        };
        let (Some(file), Some(root)) = (file, root) else {
            return (None, Restore::Done);
        };
        let read = restore::read(fs.clone(), file.clone(), root);
        let reading = cx.spawn(async move |shell, cx| {
            let session = cx.background_spawn(read).await;
            let _ = shell.update(cx, |shell, cx| shell.session_read(session, cx));
        });
        let session = SessionFile {
            writer: SessionWriter::new(fs, file),
            throttle: None,
            // Spawned before the future: shutdown holds the app borrowed.
            _quit: cx.on_app_quit(|shell, cx| {
                let write = shell.write_session(cx);
                async move {
                    if let Some(write) = write {
                        write.await;
                    }
                }
            }),
        };
        (Some(session), Restore::Loading { _read: reading })
    }

    /// Every change the session saves calls this: a switch, a tab op, a caret move. O(1).
    pub(super) fn save_session_later(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        if session.throttle.is_none() {
            session.throttle = Some(cx.spawn(async move |shell, cx| {
                cx.background_executor().timer(THROTTLE).await;
                let _ = shell.update(cx, Self::session_throttled);
            }));
        }
    }

    fn session_throttled(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            session.throttle = None;
        }
        if let Some(write) = self.write_session(cx) {
            write.detach();
        }
    }

    /// The one writer. Reads each view, sealing no undo group and forgetting no anchor.
    pub(super) fn write_session(&mut self, cx: &App) -> Option<Task<()>> {
        if !matches!(self.restore, Restore::Done) {
            return None;
        }
        let session = self.session.as_mut()?;
        let shown = &self.shown;
        let notes = self.notes.read(cx);
        let vault = notes.vault()?.read(cx);
        // The bound note, not `shown.note()`: a tab whose trash is pending saves empty.
        let tabs = self.tabs.iter().map(|tab| match tab.place() {
            None => shown.bound().map_or((None, None), |bound| {
                (Some(bound.note), Some(bound.editor.read(cx).view_state()))
            }),
            Some(parked) => (parked.note(), parked.peek(cx)),
        });
        session
            .writer
            .write(vault, tabs, self.tabs.active_index(), cx)
    }

    /// Until `Done`, the empty state says nothing: the restore may still fill it.
    pub(super) fn restore_pending(&self) -> bool {
        !matches!(self.restore, Restore::Done)
    }

    fn session_read(&mut self, session: Option<Session>, cx: &mut Context<Self>) {
        match session {
            Some(session) => {
                self.restore = Restore::Waiting(session);
                self.try_restore(cx);
            }
            None => self.finish_restore(cx),
        }
    }

    /// Level-triggered, on the read and on every vault notify. Never `is_scanned`: a failed walk sets it too.
    pub(super) fn try_restore(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            return;
        };
        let Restore::Waiting(session) = &self.restore else {
            return;
        };
        if !vault.read(cx).has_read_whole() {
            return;
        }
        let plan = self
            .holds_nothing()
            .then(|| Plan::resolve(session, vault.read(cx)))
            .flatten();
        let Some(plan) = plan else {
            self.finish_restore(cx);
            return;
        };
        // Every note, not only the active one: a tab switch binds the buffer its note already has.
        let opens: Vec<_> = self.notes.update(cx, |notes, cx| {
            plan.notes().map(|note| notes.open_note(note, cx)).collect()
        });
        let loads = cx.spawn(async move |shell, cx| {
            let buffers = futures::future::join_all(opens).await;
            let _ = shell.update_in(cx, |shell, window, cx| {
                shell.land_restore(plan, buffers, window, cx);
            });
        });
        self.restore = Restore::Landing { _loads: loads };
    }

    /// Nothing the restore would replace: one tab, empty, no open pending, nothing to reopen.
    fn holds_nothing(&self) -> bool {
        self.tabs.count() == 1
            && matches!(self.shown, Shown::Empty)
            && self.activation.is_none()
            && !self.tabs.has_closed()
    }

    fn land_restore(
        &mut self,
        plan: Plan,
        buffers: Vec<Option<Entity<Buffer>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_restore(cx);
        if !self.holds_nothing() {
            return;
        }
        let Some(Restored {
            shown,
            parked,
            active,
        }) = plan.restored(buffers)
        else {
            return;
        };
        let mut places: Vec<_> = parked
            .into_iter()
            .map(|arrival| {
                let view = arrival
                    .view
                    .map(|view| ParkedView::new(arrival.buffer, view, cx));
                Parked::new(arrival.note, view)
            })
            .collect();
        // `restored` drops the active entry: `bind` shows it, and a default one holds no anchor.
        places.insert(active, Parked::default());
        self.tabs = Tabs::restored(places, active);
        let since = self.tree_cursor(cx);
        self.bind(shown.note, shown.buffer, shown.view, since, cx);
        self.focus_note_from_root(window, cx);
        self.keep_active_tab_visible();
        cx.notify();
    }

    /// Arms the throttle: a change made while the restore was pending is saved too.
    fn finish_restore(&mut self, cx: &mut Context<Self>) {
        self.restore = Restore::Done;
        self.save_session_later(cx);
    }
}
