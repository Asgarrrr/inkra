use gpui::{
    App, AppContext as _, Context, Entity, Focusable, PromptLevel, ScrollStrategy, Subscription,
    Window,
};
use inkra_vault::{EntryId, Kind, NameProblem};

use crate::field::{Cancelled, Confirmed, NameField};

use super::{Rename, Sidebar, Trash};

/// A name being typed into the tree, holding only what confirm needs — one
/// struct for one gesture, as Zed's `EditState` is
/// (`project_panel/src/project_panel.rs:239-256`).
pub(super) struct EditState {
    /// The entry being renamed. `None` while creating — nothing exists yet,
    /// and this one `Option` is what tells the two gestures apart. Everything
    /// below is shared by both.
    pub(super) target: Option<EntryId>,
    /// The folder the new entry goes in, or the vault root. Also the folder a
    /// rename's collision check looks in, which is the one it moves within.
    parent: Option<EntryId>,
    kind: Kind,
    /// Index in `visible` where the field's row is drawn, and the indent it
    /// takes. `rebuild` moves `at` to follow the tree; `depth` is fixed at the
    /// gesture, because a new entry's indent is decided by where it will land
    /// rather than by what the listing currently holds.
    pub(super) at: usize,
    pub(super) depth: usize,
    pub(super) field: Entity<NameField>,
    /// Why the last `Confirm` was refused. Cleared by the next one.
    ///
    /// Set at confirm, not recomputed per keystroke, since Zed's live
    /// validation (`project_panel.rs:2012-2080`) amortises over a much larger
    /// surface. Must not disappear silently — the failure in Zed's own
    /// confirm, which returns wordlessly when the name is taken.
    pub(super) rejected: Option<Rejection>,
    /// Confirm and cancel. Held, because dropping a `Subscription` cancels it.
    _subscriptions: [Subscription; 2],
}

/// Why a typed name was refused.
///
/// `NameProblem` plus the one question `vault` cannot answer: whether the name
/// is taken. That needs the live listing, not a property of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rejection {
    Name(NameProblem),
    Duplicate,
}

impl Rejection {
    /// The English. `vault` states the rule; this layer owns the wording.
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Name(NameProblem::Empty) => "A name is required.",
            Self::Name(NameProblem::Separator) => "A name cannot contain / or :.",
            Self::Name(NameProblem::LeadingDot) => "A name cannot start with a dot.",
            Self::Name(NameProblem::Reserved) => "That name is reserved.",
            Self::Name(NameProblem::TooLong) => "That name is too long.",
            Self::Duplicate => "That name is already taken.",
        }
    }
}

/// The last component of a vault-relative path, as text.
///
/// Compared against what `note_file_name` produces, so the collision check sees
/// `plan` and `plan.md` as one name rather than two.
fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

impl Sidebar {
    /// Asks before deleting, then deletes.
    ///
    /// A native prompt, not an in-window one: gpui has no confirmation
    /// widget. The wording names unsaved changes specifically, because
    /// trashing takes the file as it is on disk and the editor's newer text
    /// is not recoverable from the Trash.
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
        // Folders are not deletable yet: one gesture would take every note
        // inside, which needs a prompt that counts them and the closing of
        // every buffer in the subtree.
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
                // Moved before the delete resolves, so the tree never shows a
                // selection on a row that is on its way out. The rescan the
                // delete triggers rebuilds `visible` under it.
                sidebar.selected = next;
                sidebar
                    .notes
                    .update(cx, |notes, cx| notes.trash(id, cx))
                    .detach();
            });
        })
        .detach();
    }

    /// Opens an empty field where the new entry will appear.
    ///
    /// Called by `AppShell`, never registered as an action here — see
    /// CLAUDE.md on where `cmd-n`'s dispatch is handled and why.
    ///
    /// The field goes inside the selection when it is a folder, beside it when
    /// it is a note, and at the root when nothing is selected. A selected
    /// folder is expanded first, or the field would be drawn into a subtree
    /// nobody can see.
    pub(crate) fn start_creating(
        &mut self,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.open_create(kind, cx);
        window.focus(&field.focus_handle(cx), cx);
    }

    /// `start_creating` without the focus move, which is the only part that
    /// needs a `Window`.
    pub(super) fn open_create(&mut self, kind: Kind, cx: &mut Context<Self>) -> Entity<NameField> {
        let placement = self.insertion_point(cx);
        self.open_edit(None, placement, kind, "", cx)
    }

    /// Installs the field both gestures type into, and scrolls it into view.
    /// `placement` is `(parent, row index, indent)`, as `insertion_point`
    /// returns it.
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
        // Load-bearing, not polish — see `Sidebar::scroll`.
        self.scroll.scroll_to_item(at, ScrollStrategy::Center);
        cx.notify();
        field
    }

    /// The field both gestures use, and the two subscriptions that end it.
    /// Shared because confirming and cancelling do not differ between
    /// creating and renaming — see `EditState::target`.
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

    /// Opens a field over the selected row, holding its current name.
    ///
    /// The whole name is selected, so typing replaces it and an arrow key still
    /// reaches the old one — which is what a user correcting a typo wants.
    pub(super) fn rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.open_rename(cx) {
            window.focus(&field.focus_handle(cx), cx);
        }
    }

    /// `rename` without the focus move. `None` when there is nothing to rename
    /// or a name is already being typed.
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

    /// Where a new entry goes, as `(parent, row index, indent)`.
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
            // First child of the folder, which is where the tree order puts a
            // new entry under it.
            return (Some(row.id), selected + 1, row.depth + 1);
        }
        (self.parent_of(&row), selected + 1, row.depth)
    }

    /// Validates, then creates. A refused name leaves the field open with the
    /// reason under it, rather than closing and doing nothing.
    fn confirm(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.as_ref() else {
            return;
        };
        let (target, parent, kind) = (edit.target, edit.parent, edit.kind);

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
        // The one place the two gestures diverge — everything above is shared.
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
                    // The rescan the create awaited has already rebuilt
                    // `visible`, so the row exists to be selected.
                    sidebar.selected = Some(id);
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Whether `parent` already holds an entry by this name.
    ///
    /// Answered here, not in `vault`: it is a question about the live
    /// listing, not a path property. Compared on the file name a note would
    /// actually get, so `plan` and `plan.md` collide. `target` is excluded so
    /// a same-name rename is a no-op, not a duplicate — this also covers a
    /// case-only rename.
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
            // The root: a top-level entry is one with no parent component.
            return vault.entries().any(|entry| {
                Some(entry.id) != target
                    && entry.path.depth() == 0
                    && file_name_of(entry.path.as_path()) == wanted
            });
        };
        vault.entries().any(|entry| {
            Some(entry.id) != target
                && entry.path.depth() == parent_path.depth() + 1
                && entry.path.starts_with(&parent_path)
                && file_name_of(entry.path.as_path()) == wanted
        })
    }
}
