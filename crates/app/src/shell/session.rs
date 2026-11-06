//! The session file behind the shell: throttled writes, the quit write, and the one-shot restore (restore-tabs spec, R5–R6).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{App, Context, Entity, Subscription, Task, Window};
use inkra_layout::{GroupId, Layout, Member, Side};
use inkra_tabs::Place as _;
use inkra_tabs::Tabs;
use inkra_text::Buffer;
use inkra_vault::EntryId;

use super::AppShell;
use super::group::Group;
use super::parked::Parked;
use crate::editor::ParkedView;
use crate::restore::{
    Landed, Pending, Plan, Restored, SavedGroup, SavedMember, SavedNoteTab, SavedTab, Session,
    SessionWriter, Surfaces,
};
use crate::vault::NoteStore;

/// A crash loses at most this much of the tab changes, never text: autosave writes that.
pub(super) const THROTTLE: Duration = Duration::from_secs(1);

/// Only `Done` writes: an earlier write would replace the saved tabs with the launch's one tab.
pub(super) enum Restore {
    Waiting(Pending),
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
    /// The tree and the dock groups `read` holds, its notes still to load; docks rule 6's with none.
    pub(super) fn restored_groups(
        read: Option<Session>,
    ) -> (Layout, HashMap<GroupId, Group>, Option<Pending>) {
        let Surfaces {
            layout,
            docks,
            pending,
        } = Surfaces::resolve(read);
        let groups = layout
            .center_groups()
            .into_iter()
            .map(|group| (group, Group::new()))
            .chain(
                docks
                    .into_iter()
                    .map(|(group, views, active)| (group, Group::holding(&views, active))),
            )
            .collect();
        (layout, groups, pending)
    }

    /// A vault launch with a session file keeps its writer; it waits on the walk only with a session read.
    /// Spawns nothing: the vault's notify tries the restore and arms the throttle.
    pub(super) fn open_session(
        file: Option<PathBuf>,
        pending: Option<Pending>,
        notes: &Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> (Option<SessionFile>, Restore) {
        let Some(file) = file else {
            return (None, Restore::Done);
        };
        let session = SessionFile {
            writer: SessionWriter::new(notes.read(cx).fs(), file),
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
        (
            Some(session),
            pending.map_or(Restore::Done, Restore::Waiting),
        )
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
        self.session.as_ref()?;
        let saved = self.saved_session(cx)?;
        self.session.as_mut()?.writer.write(&saved, cx)
    }

    fn saved_session(&self, cx: &App) -> Option<Session> {
        let notes = self.notes.read(cx);
        let root = notes.vault()?.read(cx).root().to_path_buf();
        let path_of = |id| notes.vault_path(id, cx);
        let last = self.layout.last_center();
        let active = self.layout.groups().position(|group| group == last)?;
        let [left, right] = [Side::Left, Side::Right].map(|side| {
            let dock = self.layout.dock(side)?;
            Some(self.saved_member(dock, &path_of, cx))
        });
        let shown = [Side::Left, Side::Right].map(|side| self.layout.is_shown(side));
        let center = self.saved_member(self.layout.center(), &path_of, cx);
        Some(Session::new(root, active, center, left, right, shown))
    }

    fn saved_member(
        &self,
        member: &Member,
        path_of: &impl Fn(EntryId) -> Option<PathBuf>,
        cx: &App,
    ) -> SavedMember {
        match member {
            Member::Group(group) => SavedMember::Group(self.saved_group(*group, path_of, cx)),
            Member::Split {
                axis,
                children,
                flexes,
            } => SavedMember::Split {
                split: (*axis).into(),
                flexes: flexes.clone(),
                children: children
                    .iter()
                    .map(|child| self.saved_member(child, path_of, cx))
                    .collect(),
            },
        }
    }

    fn saved_group(
        &self,
        group: GroupId,
        path_of: &impl Fn(EntryId) -> Option<PathBuf>,
        cx: &App,
    ) -> SavedGroup {
        let group = self.group(group);
        let tabs = group.tabs.iter().map(|tab| {
            if let Some(view) = tab.place().map_or_else(|| group.shown.view(), Parked::view) {
                return SavedTab::View {
                    view: view.name().to_owned(),
                };
            }
            // The bound note, not `shown.note()`: a tab whose trash is pending saves empty.
            let (note, view) = match tab.place() {
                None => group.shown.bound().map_or((None, None), |bound| {
                    (Some(bound.note), Some(bound.editor.read(cx).view_state()))
                }),
                Some(parked) => (parked.note(), parked.peek(cx)),
            };
            SavedTab::Note(SavedNoteTab::new(note.and_then(path_of), view.as_ref()))
        });
        SavedGroup {
            tabs: tabs.collect(),
            active: group.tabs.active_index(),
        }
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
        let Restore::Waiting(pending) = &self.restore else {
            return;
        };
        if !vault.read(cx).has_read_whole() {
            return;
        }
        let plan = self
            .holds_nothing()
            .then(|| Plan::resolve(pending, vault.read(cx)))
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

    /// Nothing the restore would replace, in any centre group.
    fn holds_nothing(&self) -> bool {
        self.layout
            .center_groups()
            .into_iter()
            .all(|group| self.group(group).holds_nothing())
    }

    /// A planned group the user closed meanwhile is skipped.
    fn land_restore(
        &mut self,
        plan: Plan,
        buffers: Vec<Option<Entity<Buffer>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.holds_nothing() {
            self.finish_restore(cx);
            return;
        }
        let Landed { groups, active } = plan.restored(buffers);
        for (group, restored) in groups {
            if self.groups.contains_key(&group) {
                self.land_group(group, restored, cx);
            }
        }
        self.finish_restore(cx);
        let target = active
            .into_iter()
            .find(|group| self.groups.contains_key(group))
            .unwrap_or(self.layout.center_groups()[0]);
        if target != self.layout.last_center() {
            self.switch_group(target, cx);
        }
        if cfg!(debug_assertions) {
            self.check_layout(cx);
        }
        self.focus_note_from_root(window, cx);
        cx.notify();
    }

    fn land_group(
        &mut self,
        group: GroupId,
        Restored {
            shown,
            parked,
            active,
        }: Restored<Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) {
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
        self.group_mut(group).tabs = Tabs::restored(places, active);
        let since = self.tree_cursor(cx);
        self.bind(group, shown.note, shown.buffer, shown.view, since, cx);
        self.keep_active_tab_visible(group);
    }

    /// Removes the centre groups left holding nothing, then arms the throttle: a change made while
    /// the restore was pending is saved too.
    fn finish_restore(&mut self, cx: &mut Context<Self>) {
        self.restore = Restore::Done;
        self.drop_empty_groups(cx);
        if cfg!(debug_assertions) {
            self.check_layout(cx);
        }
        self.save_session_later(cx);
    }

    /// The last centre group stays, with its empty tab (L3).
    fn drop_empty_groups(&mut self, cx: &mut Context<Self>) {
        for group in self.layout.center_groups() {
            if !self.layout.removable(group) {
                return;
            }
            if !self.group(group).holds_nothing() {
                continue;
            }
            let was_last = group == self.layout.last_center();
            let Some(heir) = self.layout.remove(group) else {
                panic!("a centre group has an heir");
            };
            self.groups.remove(&group);
            if was_last {
                self.enter_group(heir, cx);
            }
        }
    }
}
