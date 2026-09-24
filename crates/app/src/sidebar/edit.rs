use gpui::{
    App, AppContext as _, Context, Entity, Focusable, PromptLevel, ScrollStrategy, Subscription,
    Window,
};
use inkra_vault::{Entry, EntryId, Kind, NameProblem};

use crate::field::{Cancelled, Confirmed, NameField};

use super::{Rename, Sidebar, Trash};

pub(super) struct EditState {
    /// `None` while creating: this is what tells the two gestures apart.
    pub(super) target: Option<EntryId>,
    parent: Option<EntryId>,
    kind: Kind,
    pub(super) at: usize,
    pub(super) depth: usize,
    pub(super) field: Entity<NameField>,
    /// Must not disappear silently on a refusal — Zed's own confirm does that.
    pub(super) rejected: Option<Rejection>,
    _subscriptions: [Subscription; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rejection {
    Name(NameProblem),
    Duplicate,
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
        }
    }
}

/// Compared against `note_file_name`'s output, so `plan`, `plan.md` and `plan.MD` collide as one name.
fn file_name_of(entry: &Entry) -> String {
    let name = entry
        .path
        .as_path()
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    match entry.kind {
        Kind::Note => inkra_vault::note_file_name(&name),
        Kind::Dir => name,
    }
}

impl Sidebar {
    /// Trashing takes the file as it is on disk; the editor's newer text is not recoverable.
    pub(super) fn trash(&mut self, _: &Trash, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        let Some(id) = self.selected else {
            return;
        };
        let Some(row) = self.visible.iter().find(|row| row.id == id) else {
            return;
        };
        // Folders are not deletable yet: one gesture would take every note inside.
        if row.kind == Kind::Dir {
            return;
        }

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

        let next = self.after_deleting(id);
        cx.spawn(async move |sidebar, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = sidebar.update(cx, |sidebar, cx| {
                // Moved before the delete resolves, so selection never points at a row on its way out.
                sidebar.selected = next;
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
        let placement = self.insertion_point(cx);
        self.open_edit(None, placement, kind, "", cx)
    }

    fn open_edit(
        &mut self,
        target: Option<EntryId>,
        (parent, at, depth): (Option<EntryId>, usize, usize),
        kind: Kind,
        initial: &str,
        cx: &mut Context<Self>,
    ) -> Entity<NameField> {
        let (field, subscriptions) = Self::open_field(initial, cx);
        self.editing = Some(EditState {
            target,
            parent,
            kind,
            at,
            depth,
            field: field.clone(),
            rejected: None,
            _subscriptions: subscriptions,
        });
        // Load-bearing, not polish — see `Sidebar::scroll`'s field doc.
        self.scroll.scroll_to_item(at, ScrollStrategy::Center);
        cx.notify();
        field
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
            sidebar.editing = None;
            cx.notify();
        });
        (field, [confirmed, cancelled])
    }

    pub(super) fn rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.open_rename(cx) {
            window.focus(&field.focus_handle(cx), cx);
        }
    }

    pub(super) fn open_rename(&mut self, cx: &mut Context<Self>) -> Option<Entity<NameField>> {
        if self.editing.is_some() {
            return None;
        }
        let id = self.selected?;
        let at = self.index_of(id)?;
        let row = self.visible[at].clone();
        let parent = self.parent_of(&row);
        Some(self.open_edit(Some(id), (parent, at, row.depth), row.kind, &row.name, cx))
    }

    pub(super) fn insertion_point(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Option<EntryId>, usize, usize) {
        let Some(selected) = self.selected.and_then(|id| self.index_of(id)) else {
            return (None, self.visible.len(), 0);
        };
        let row = self.visible[selected].clone();
        if row.kind == Kind::Dir {
            if !self.expanded.contains(&row.id) {
                self.toggle(row.id, cx);
            }
            return (Some(row.id), selected + 1, row.depth + 1);
        }
        (self.parent_of(&row), selected + 1, row.depth)
    }

    /// A refused name leaves the field open with the reason under it.
    fn confirm(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.as_ref() else {
            return;
        };
        let (target, parent, kind) = (edit.target, edit.parent, edit.kind);
        // Before `check_name`: a note made outside the app may hold a name it now refuses.
        if self.is_current_name(target, name, cx) {
            self.editing = None;
            cx.notify();
            return;
        }

        let rejected = inkra_vault::check_name(name)
            .map(Rejection::Name)
            .or_else(|| {
                self.taken(target, parent, kind, name, cx)
                    .then_some(Rejection::Duplicate)
            });
        if let Some(rejected) = rejected {
            if let Some(edit) = self.editing.as_mut() {
                edit.rejected = Some(rejected);
            }
            cx.notify();
            return;
        }

        self.editing = None;
        let name = name.to_owned();
        if let Some(id) = target {
            self.notes
                .update(cx, |notes, cx| notes.rename(id, &name, cx))
                .detach();
        } else {
            let created = self
                .notes
                .update(cx, |notes, cx| notes.create(parent, kind, &name, cx));
            cx.spawn(async move |sidebar, cx| {
                let Some(id) = created.await else { return };
                let _ = sidebar.update(cx, |sidebar, cx| {
                    sidebar.selected = Some(id);
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn is_current_name(&self, target: Option<EntryId>, name: &str, cx: &App) -> bool {
        let Some(vault) = self.notes.read(cx).vault() else {
            return false;
        };
        target
            .and_then(|id| vault.read(cx).entry(id))
            .is_some_and(|entry| entry.path.display_name() == name.trim())
    }

    /// `target` is excluded so a same-name (or case-only) rename is a no-op, not a duplicate.
    fn taken(
        &self,
        target: Option<EntryId>,
        parent: Option<EntryId>,
        kind: Kind,
        name: &str,
        cx: &App,
    ) -> bool {
        let wanted = match kind {
            Kind::Note => inkra_vault::note_file_name(name),
            Kind::Dir => name.trim().to_owned(),
        };
        let Some(vault) = self.notes.read(cx).vault() else {
            return false;
        };
        let vault = vault.read(cx);
        let Some(parent_path) = parent
            .and_then(|id| vault.entry(id))
            .map(|e| e.path.clone())
        else {
            return vault.entries().any(|entry| {
                Some(entry.id) != target && entry.path.depth() == 0 && file_name_of(entry) == wanted
            });
        };
        vault.entries().any(|entry| {
            Some(entry.id) != target
                && entry.path.depth() == parent_path.depth() + 1
                && entry.path.starts_with(&parent_path)
                && file_name_of(entry) == wanted
        })
    }
}
