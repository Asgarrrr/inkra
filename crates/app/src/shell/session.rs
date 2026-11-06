//! The session file behind the shell: throttled writes, the quit write, and the one-shot restore (restore-tabs spec, R5–R6).

use std::collections::HashMap;
use std::time::Duration;

use gpui::{App, Context, Entity, Subscription, Task, Window};
use inkra_layout::{GroupId, Layout, Side};
use inkra_tabs::Place as _;
use inkra_tabs::Tabs;
use inkra_text::Buffer;

use super::group::Group;
use super::parked::Parked;
use super::{AppShell, Shown};
use crate::editor::ParkedView;
use crate::restore::{Opened, Plan, Restored, SavedDock, SavedDocks, Session, SessionWriter};
use crate::vault::NoteStore;

/// A crash loses at most this much of the tab changes, never text: autosave writes that.
pub(super) const THROTTLE: Duration = Duration::from_secs(1);

/// Only `Done` writes: an earlier write would replace the saved tabs with the launch's one tab.
pub(super) enum Restore {
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
    /// The centre's one empty group and the dock groups `saved` holds; docks rule 6's with none.
    pub(super) fn dock_groups(saved: Option<&SavedDocks>) -> (Layout, HashMap<GroupId, Group>) {
        let mut layout = Layout::default();
        let mut groups = HashMap::from([(layout.last_center(), Group::new())]);
        let docks = saved.map_or_else(|| SavedDocks::default().views(), SavedDocks::views);
        for (side, (views, active, shown)) in [Side::Left, Side::Right].into_iter().zip(docks) {
            let Some(group) = layout.dock(side) else {
                panic!("the default layout has a group in each dock");
            };
            if views.is_empty() {
                layout.remove(group);
            } else {
                groups.insert(group, Group::holding(&views, active));
            }
            if !shown {
                layout.toggle(side);
            }
        }
        (layout, groups)
    }

    /// A vault launch with a session file keeps its writer; it waits on the walk only with a session read.
    /// Spawns nothing: the vault's notify tries the restore and arms the throttle.
    pub(super) fn open_session(
        session: Option<Opened>,
        notes: &Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> (Option<SessionFile>, Restore) {
        let (fs, has_vault) = {
            let notes = notes.read(cx);
            (notes.fs(), notes.vault().is_some())
        };
        let Some(Opened {
            file,
            session: read,
        }) = session.filter(|_| has_vault)
        else {
            return (None, Restore::Done);
        };
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
        (Some(session), read.map_or(Restore::Done, Restore::Waiting))
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
        let docks = self
            .panes
            .as_ref()
            .map(|_| SavedDocks::new(self.saved_dock(Side::Left), self.saved_dock(Side::Right)));
        let session = self.session.as_mut()?;
        let center = &self.groups[&self.layout.last_center()];
        let shown = &center.shown;
        let notes = self.notes.read(cx);
        let vault = notes.vault()?.read(cx);
        // The bound note, not `shown.note()`: a tab whose trash is pending saves empty.
        let tabs = center.tabs.iter().map(|tab| match tab.place() {
            None => shown.bound().map_or((None, None), |bound| {
                (Some(bound.note), Some(bound.editor.read(cx).view_state()))
            }),
            Some(parked) => (parked.note(), parked.peek(cx)),
        });
        session.writer.write(
            vault,
            tabs,
            |id| notes.vault_path(id, cx),
            center.tabs.active_index(),
            docks,
            cx,
        )
    }

    /// An empty dock saves no view, at index 0.
    fn saved_dock(&self, side: Side) -> SavedDock {
        let active = self
            .layout
            .dock(side)
            .map_or(0, |group| self.group(group).tabs.active_index());
        SavedDock::new(self.dock_views(side), active, self.layout.is_shown(side))
    }

    /// Until `Done`, the empty state says nothing: the restore may still fill it.
    pub(super) fn restore_pending(&self) -> bool {
        !matches!(self.restore, Restore::Done)
    }

    /// Level-triggered, on every vault notify. Never `is_scanned`: a failed walk sets it too.
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

    /// Nothing the restore would replace: one group, one tab, empty, no open pending, nothing to reopen.
    fn holds_nothing(&self) -> bool {
        let center = self.center();
        self.layout.center_groups().len() == 1
            && center.tabs.count() == 1
            && matches!(center.shown, Shown::Empty)
            && center.activation.is_none()
            && !center.tabs.has_closed()
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
        let center = self.layout.last_center();
        self.group_mut(center).tabs = Tabs::restored(places, active);
        let since = self.tree_cursor(cx);
        self.bind(center, shown.note, shown.buffer, shown.view, since, cx);
        self.focus_note_from_root(window, cx);
        self.keep_active_tab_visible(center);
        cx.notify();
    }

    /// Arms the throttle: a change made while the restore was pending is saved too.
    fn finish_restore(&mut self, cx: &mut Context<Self>) {
        self.restore = Restore::Done;
        self.save_session_later(cx);
    }
}
