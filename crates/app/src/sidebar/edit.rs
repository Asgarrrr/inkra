use gpui::{
    App, AppContext as _, Context, Entity, Focusable, PromptLevel, ScrollStrategy, Subscription,
    Window,
};
use std::io::ErrorKind;

use inkra_vault::{EntryId, Kind, NameProblem};

use crate::field::{Cancelled, Confirmed, NameField};

use super::{Rename, Sidebar, Trash};

pub(super) struct EditState {
    parent: Option<EntryId>,
    kind: Kind,
    pub(super) depth: usize,
    pub(super) field: Entity<NameField>,
    /// Must not disappear silently on a refusal — Zed's own confirm does that.
    pub(super) rejected: Option<Rejection>,
    /// A create in flight keeps the field open and ignores a second Enter.
    creating: bool,
    _subscriptions: [Subscription; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rejection {
    Name(NameProblem),
    Duplicate,
    Refused,
}

impl Rejection {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Name(NameProblem::Empty) => "A name is required.",
            Self::Name(NameProblem::Separator) => "A name cannot contain / or :.",
            Self::Name(NameProblem::LeadingDot) => "A name cannot start with a dot.",
            Self::Name(NameProblem::Reserved) => "That name is reserved.",
            Self::Name(NameProblem::LinkSyntax) => "A name cannot contain #, |, ^, [ or ].",
            Self::Name(NameProblem::TooLong) => "That name is too long.",
            Self::Duplicate => "That name is already taken.",
            Self::Refused => "The file could not be created.",
        }
    }
}

impl Sidebar {
    /// Trashing takes the file as it is on disk; the editor's newer text is not recoverable.
    pub(super) fn trash(&mut self, _: &Trash, window: &mut Window, cx: &mut Context<Self>) {
        if self.rows.edit().is_some() {
            return;
        }
        let Some(id) = self.selected else {
            return;
        };
        let visible = self.rows.visible();
        let Some(at) = visible.index_of(id) else {
            return;
        };
        // Folders are not deletable yet: one gesture would take every note inside.
        if visible.entry(at).kind == Kind::Dir {
            return;
        }
        let row = visible.row(at);

        let message = format!("Move \u{201c}{}\u{201d} to the Trash?", row.name);
        let detail = self
            .notes
            .read(cx)
            .is_dirty(id, cx)
            .then_some("It has unsaved changes, which will be lost.");
        let answer = window.prompt(
            PromptLevel::Info,
            &message,
            detail,
            &["Move to Trash", "Cancel"],
            cx,
        );

        let next = visible.neighbour(id);
        cx.spawn(async move |sidebar, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = sidebar.update(cx, |sidebar, cx| {
                // Moved before the delete resolves, so selection never points at a row on its way out.
                sidebar.select(next);
                sidebar
                    .notes
                    .update(cx, |notes, cx| notes.trash(id, cx))
                    .detach();
            });
        })
        .detach();
    }

    /// Called by `AppShell`, never registered as an action here.
    pub(crate) fn start_creating(
        &mut self,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.open_create(kind, cx);
        window.focus(&field.focus_handle(cx), cx);
    }

    pub(super) fn open_create(&mut self, kind: Kind, cx: &mut Context<Self>) -> Entity<NameField> {
        let (parent, item, depth) = self.insertion_point(cx);
        let edit = Self::edit_state(parent, kind, depth, "", cx);
        let field = edit.field.clone();
        self.rows.insert(item, edit);
        self.show_field(item, cx);
        field
    }

    fn edit_state(
        parent: Option<EntryId>,
        kind: Kind,
        depth: usize,
        initial: &str,
        cx: &mut Context<Self>,
    ) -> EditState {
        let (field, subscriptions) = Self::open_field(initial, cx);
        EditState {
            parent,
            kind,
            depth,
            field,
            rejected: None,
            creating: false,
            _subscriptions: subscriptions,
        }
    }

    fn show_field(&self, item: usize, cx: &mut Context<Self>) {
        // Load-bearing, not polish — see `Sidebar::scroll`'s field doc.
        self.scroll.scroll_to_item(item, ScrollStrategy::Center);
        cx.notify();
    }

    pub(super) fn open_field(
        initial: &str,
        cx: &mut Context<Self>,
    ) -> (Entity<NameField>, [Subscription; 2]) {
        let initial = initial.to_owned();
        let field = cx.new(|cx| NameField::new(&initial, cx));
        let confirmed = cx.subscribe(&field, |sidebar, _, event: &Confirmed, cx| {
            sidebar.confirm(&event.0, cx);
        });
        let cancelled = cx.subscribe(&field, |sidebar, _, _: &Cancelled, cx| {
            sidebar.cancel_edit(cx);
        });
        (field, [confirmed, cancelled])
    }

    /// Closes the name field without acting on it: a half-typed name creates and renames nothing.
    pub(crate) fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if self.rows.close().is_some() {
            cx.notify();
        }
    }

    pub(super) fn rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.open_rename(cx) {
            window.focus(&field.focus_handle(cx), cx);
        }
    }

    pub(super) fn open_rename(&mut self, cx: &mut Context<Self>) -> Option<Entity<NameField>> {
        if self.rows.edit().is_some() {
            return None;
        }
        let id = self.selected?;
        let visible = self.rows.visible();
        let at = visible.index_of(id)?;
        let row = visible.row(at);
        let parent = visible.parent_of(at);
        let edit = Self::edit_state(parent, row.kind, row.depth, &row.name, cx);
        let field = edit.field.clone();
        let item = self.rows.replace(id, edit)?;
        self.show_field(item, cx);
        Some(field)
    }

    pub(super) fn insertion_point(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Option<EntryId>, usize, usize) {
        let visible = self.rows.visible();
        let Some(selected) = self.selected.and_then(|id| visible.index_of(id)) else {
            return (None, visible.len(), 0);
        };
        let entry = visible.entry(selected);
        let (id, depth) = (entry.id, entry.path.depth());
        if entry.kind == Kind::Dir {
            if !self.expanded.contains(&id) {
                self.toggle(id, cx);
            }
            return (Some(id), selected + 1, depth + 1);
        }
        (visible.parent_of(selected), selected + 1, depth)
    }

    /// A refused name leaves the field open with the reason under it.
    fn confirm(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.rows.edit() else {
            return;
        };
        if edit.creating {
            return;
        }
        let (target, parent, kind) = (self.rows.target(), edit.parent, edit.kind);
        // Before `check_name`: a note made outside the app may hold a name it now refuses.
        if self.is_current_name(target, name, cx) {
            self.rows.close();
            cx.notify();
            return;
        }

        let rejected = inkra_vault::check_name(name)
            .map(Rejection::Name)
            .or_else(|| {
                self.vault
                    .read(cx)
                    .name_taken(parent, kind, name, target)
                    .then_some(Rejection::Duplicate)
            });
        if let Some(rejected) = rejected {
            if let Some(edit) = self.rows.edit_mut() {
                edit.rejected = Some(rejected);
            }
            cx.notify();
            return;
        }

        let name = name.to_owned();
        if let Some(id) = target {
            self.rows.close();
            self.notes
                .update(cx, |notes, cx| notes.rename(id, &name, cx))
                .detach();
        } else {
            if let Some(edit) = self.rows.edit_mut() {
                edit.creating = true;
                edit.rejected = None;
            }
            let created = self
                .notes
                .update(cx, |notes, cx| notes.create(parent, kind, &name, cx));
            cx.spawn(async move |sidebar, cx| {
                let created = created.await;
                let _ = sidebar.update(cx, |sidebar, cx| {
                    sidebar.created(created, cx);
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Escape during a create closed the field already: `creating` tells the field of this create from a later one.
    fn created(&mut self, created: inkra_vault::Result<Option<EntryId>>, cx: &mut Context<Self>) {
        let creating = self.rows.edit().is_some_and(|edit| edit.creating);
        match created {
            Ok(id) => {
                if creating {
                    self.rows.close();
                }
                if id.is_some() {
                    self.select(id);
                }
            }
            Err(error) => {
                log::warn!("create: {error}");
                if let Some(edit) = self.rows.edit_mut().filter(|edit| edit.creating) {
                    edit.creating = false;
                    edit.rejected = Some(if error.io_kind() == Some(ErrorKind::AlreadyExists) {
                        Rejection::Duplicate
                    } else {
                        Rejection::Refused
                    });
                }
            }
        }
        cx.notify();
    }

    fn is_current_name(&self, target: Option<EntryId>, name: &str, cx: &App) -> bool {
        target
            .and_then(|id| self.vault.read(cx).entry(id))
            .is_some_and(|entry| entry.path.display_name() == name.trim())
    }
}
