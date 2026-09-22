//! The vault tree: a linear walk over path-ordered entries, never a per-frame
//! tree rebuild. `Vault::entries` yields a directory's descendants in a
//! contiguous run after it, so collapsing one is a forward skip.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, PromptLevel, Render,
    ScrollStrategy, Styled, Subscription, UniformListScrollHandle, Window, actions, div, px,
    uniform_list,
};
use inkra_vault::{EntryId, Kind, NameProblem};

use crate::field::{Cancelled, Confirmed, NameField};
use crate::theme;
use crate::vault::NoteStore;

actions!(
    sidebar,
    [
        /// Moves the selected note to the platform trash, after confirming.
        ///
        /// Reads `Sidebar::selected`, so it has a target only while the tree
        /// drives it. A command palette invoking this with nothing selected
        /// would need the action to carry an `Option<EntryId>` instead.
        Trash,
        SelectPrev,
        SelectNext,
        /// Opens a field over the selected row, seeded with its current name.
        ///
        /// Reads `Sidebar::selected`, like `Trash`.
        Rename,
    ]
);

actions!(
    vault_tree,
    [
        /// Opens a field for a new note beside the tree's selection.
        ///
        /// Bound at the `AppShell` context and handled there, so it fires with
        /// the editor focused. `Sidebar::start_creating` is what it calls.
        NewNote,
        /// The same, for a folder.
        NewFolder,
    ]
);

/// The left gutter every row starts from, and what each level of nesting adds.
const INDENT_BASE: Pixels = px(8.);
/// See [`INDENT_BASE`].
const INDENT_STEP: Pixels = px(12.);

/// Where the note goes is `AppShell`'s to decide, so any widget that can open
/// one — a tab bar, a backlinks panel — plugs in by emitting this, with no
/// change here. A struct, not a one-variant enum: `EventEmitter<E>` is generic
/// over the event type (`gpui/src/gpui.rs:296`), so a second signal is a
/// second type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteActivated(pub(crate) EntryId);

impl EventEmitter<NoteActivated> for Sidebar {}

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    /// Expansion is UI state, never vault state.
    expanded: HashSet<EntryId>,
    /// What a command acts on. UI state like `expanded`, and not the same thing
    /// as `NoteStore::current`: that one names the note the editor is showing,
    /// and a user can select a row without opening it.
    selected: Option<EntryId>,
    /// Focused so the tree's own bindings resolve. Without a tracked handle the
    /// `Sidebar` key context is in no dispatch node, and every binding scoped
    /// to it would be excluded from matching.
    focus_handle: FocusHandle,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
    /// A name being typed, and where its row sits. `None` the rest of the time.
    editing: Option<EditState>,
    /// Scrolls the field's row into view when one opens.
    ///
    /// Load-bearing, not polish. `uniform_list` renders only the visible range
    /// (`elements/uniform_list.rs:479-489`), and gpui does not blur a focused
    /// handle whose element stopped being drawn: dispatch falls back to the
    /// root node (`window.rs:6244-6251`), so every keystroke — the characters,
    /// Confirm, Cancel — would vanish with no panic and no feedback.
    scroll: UniformListScrollHandle,
    /// Rebuilds `visible` when the vault's contents change. Never read; holding
    /// it is the point, because dropping a `Subscription` cancels it.
    ///
    /// Without this the cache is rebuilt only by `new` and `toggle`, so a file
    /// that appears in the vault while the window is open — the conflict file a
    /// clobbered write preserves is the only one today — never reaches a row.
    /// The vault's own `notify` had no subscriber at all.
    _vault_changed: Option<Subscription>,
}

#[derive(Clone)]
struct Row {
    id: EntryId,
    depth: usize,
    kind: Kind,
    name: String,
}

/// A name being typed into the tree.
///
/// One struct for one gesture, as Zed's `EditState` is
/// (`project_panel/src/project_panel.rs:239-256`). It holds only what the
/// confirm needs, because the text itself lives in the field.
struct EditState {
    /// The entry being renamed. `None` while creating — nothing exists yet, and
    /// this one `Option` is what tells the two gestures apart, as Zed's
    /// `leaf_entry_id` does (`project_panel.rs:252-256`). Everything below is
    /// shared by both.
    target: Option<EntryId>,
    /// The folder the new entry goes in, or the vault root. Also the folder a
    /// rename's collision check looks in, which is the one it moves within.
    parent: Option<EntryId>,
    kind: Kind,
    /// Index in `visible` where the field's row is drawn, and the indent it
    /// takes. `rebuild` moves `at` to follow the tree; `depth` is fixed at the
    /// gesture, because a new entry's indent is decided by where it will land
    /// rather than by what the listing currently holds.
    at: usize,
    depth: usize,
    field: Entity<NameField>,
    /// Why the last `Confirm` was refused. Cleared by the next one.
    ///
    /// Set at confirm rather than recomputed per keystroke. Zed validates live
    /// (`project_panel.rs:2012-2080`), which amortises over a much larger
    /// surface than one gesture ended by one keystroke. What the message must
    /// not do is disappear silently, which is the failure Zed's own confirm has
    /// — it returns without a word when the name is taken.
    rejected: Option<Rejection>,
    /// Confirm and cancel. Held, because dropping a `Subscription` cancels it.
    _subscriptions: [Subscription; 2],
}

/// Why a typed name was refused.
///
/// `NameProblem` plus the one question `vault` cannot answer: whether the name
/// is taken. That needs the live listing, not a property of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rejection {
    Name(NameProblem),
    Duplicate,
}

impl Rejection {
    /// The English. `vault` states the rule; this layer owns the wording.
    fn message(self) -> &'static str {
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
    pub(crate) fn new(notes: Entity<NoteStore>, cx: &mut Context<Self>) -> Self {
        let vault = notes.read(cx).vault().cloned();
        let watched = vault.map(|vault| cx.observe(&vault, |this, _, cx| this.rebuild(cx)));
        let mut sidebar = Self {
            notes,
            expanded: HashSet::new(),
            selected: None,
            focus_handle: cx.focus_handle(),
            visible: Vec::new(),
            editing: None,
            scroll: UniformListScrollHandle::default(),
            _vault_changed: watched,
        };
        sidebar.rebuild(cx);
        sidebar
    }

    /// `RelPath::starts_with` is component-wise, so the skip past a collapsed
    /// directory never swallows the sibling `Projects.md`.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            // Unreachable today: the vault is fixed at construction.
            cx.notify();
            return;
        };
        // Collected, not iterated: the walk borrows `cx`, which `notify` needs.
        let entries: Vec<_> = vault.read(cx).entries().cloned().collect();

        let mut i = 0;
        while i < entries.len() {
            let entry = &entries[i];
            self.visible.push(Row {
                id: entry.id,
                depth: entry.path.depth(),
                kind: entry.kind,
                name: entry.path.display_name(),
            });
            i += 1;
            if entry.kind == Kind::Dir && !self.expanded.contains(&entry.id) {
                while i < entries.len() && entries[i].path.starts_with(&entry.path) {
                    i += 1;
                }
            }
        }
        // A field open across this walk has to end up on a row the list still
        // draws: an unrendered row is in no dispatch node, so gpui falls back
        // to the root (`window.rs:6244-6251`) and every keystroke, `escape`
        // included, vanishes with no panic and no feedback.
        //
        // A rename follows its row by identity, so a walk that reordered the
        // tree keeps the field over the entry it names, and a target deleted
        // from outside ends the gesture. A create has no target and only needs
        // its index to stay in range.
        if let Some(target) = self.editing.as_ref().map(|edit| edit.target) {
            let at = match target {
                Some(id) => self.index_of(id),
                None => self
                    .editing
                    .as_ref()
                    .map(|edit| edit.at.min(self.visible.len())),
            };
            match (at, self.editing.as_mut()) {
                (Some(at), Some(edit)) => {
                    edit.at = at;
                    self.scroll.scroll_to_item(at, ScrollStrategy::Center);
                }
                _ => self.editing = None,
            }
        }
        cx.notify();
    }

    fn toggle(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(cx);
    }

    /// Expansion is the sidebar's own state; where a note opens is not. The
    /// sidebar does not know an editor exists.
    fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.selected = Some(row.id);
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(NoteActivated(row.id));
    }

    /// Moves the selection by one visible row, clamped at both ends rather than
    /// wrapping: a tree that jumps from the last note back to the first loses
    /// the user's place.
    ///
    /// Does nothing while a name is being typed. The field is a child of this
    /// view's dispatch node and claims neither arrow, so without the guard the
    /// key bubbles up here and moves the selection out from under the gesture
    /// in flight. `trash` and `rename` carry the same guard, and `cmd-backspace`
    /// is the one that bites: it is the macOS delete-to-line-start gesture, so
    /// a user correcting a name would be asked whether to delete the row.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.editing.is_some() || self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let next = match self.selected.and_then(|id| self.index_of(id)) {
            Some(index) => index.saturating_add_signed(delta).min(last),
            // Nothing selected: down starts at the top, up at the bottom.
            None if delta > 0 => 0,
            None => last,
        };
        self.selected = self.visible.get(next).map(|row| row.id);
        cx.notify();
    }

    /// Selects a row without a window, for a test that has none.
    #[cfg(test)]
    pub(crate) fn select_for_test(&mut self, id: EntryId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    /// Opens a rename field without focusing it. `#[cfg(test)]`: the real entry
    /// point needs a `Window`, and these tests exercise the rebuild rather than
    /// the gesture.
    #[cfg(test)]
    pub(crate) fn rename_for_test(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        let Some(at) = self.index_of(id) else { return };
        let row = self.visible[at].clone();
        let parent = self.parent_of(&row);
        let (field, subscriptions) = Self::open_field(&row.name, cx);
        self.editing = Some(EditState {
            target: Some(id),
            parent,
            kind: row.kind,
            at,
            depth: row.depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
    }

    /// The same, for a create.
    #[cfg(test)]
    pub(crate) fn create_for_test(&mut self, cx: &mut Context<Self>) {
        let (parent, at, depth) = self.insertion_point(cx);
        let (field, subscriptions) = Self::open_field("", cx);
        self.editing = Some(EditState {
            target: None,
            parent,
            kind: Kind::Note,
            at,
            depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
    }

    /// Where the field's row sits, for the shrink tests.
    #[cfg(test)]
    pub(crate) fn field_row(&self) -> usize {
        self.editing.as_ref().map_or(0, |edit| edit.at)
    }

    /// Whether a name is being typed. `#[cfg(test)]` for the same reason as
    /// `selected`: nothing in the UI asks, and an unread method fails
    /// `-D warnings`.
    #[cfg(test)]
    pub(crate) fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// Why the last confirm was refused, as the user would read it. Asserting
    /// the sentence rather than the variant is what stops a rejection being
    /// reported with the wrong reason.
    #[cfg(test)]
    pub(crate) fn rejection_message(&self) -> Option<&'static str> {
        self.editing
            .as_ref()
            .and_then(|edit| edit.rejected)
            .map(Rejection::message)
    }

    /// The row a command would act on. `#[cfg(test)]` and an accessor rather
    /// than a public field, following `NoteStore::current`: nothing in the UI
    /// asks, and an unread method fails `-D warnings`.
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<EntryId> {
        self.selected
    }

    fn index_of(&self, id: EntryId) -> Option<usize> {
        self.visible.iter().position(|row| row.id == id)
    }

    /// Asks before deleting, then deletes.
    ///
    /// A native prompt rather than an in-window one: gpui has no confirmation
    /// widget, and the platform dialog is both free and the thing a macOS user
    /// expects. The wording names the unsaved changes that are about to go,
    /// because trashing takes the file as it is on disk and the editor's newer
    /// text is not recoverable from the Trash.
    fn trash(&mut self, _: &Trash, window: &mut Window, cx: &mut Context<Self>) {
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

    /// Where the selection lands once `id` is gone: the next row, else the one
    /// before it, else nothing. Computed before the delete, because afterwards
    /// the row it is relative to no longer exists.
    fn after_deleting(&self, id: EntryId) -> Option<EntryId> {
        let index = self.index_of(id)?;
        self.visible
            .get(index + 1)
            .or_else(|| {
                index
                    .checked_sub(1)
                    .and_then(|before| self.visible.get(before))
            })
            .map(|row| row.id)
    }

    /// Opens an empty field where the new entry will appear.
    ///
    /// Called by `AppShell`, never registered as an action here: `NewNote` and
    /// `NewFolder` bind at the `AppShell` context so they fire with the editor
    /// focused, and dispatch runs from the focused leaf up its own ancestors
    /// (`gpui/src/window.rs:6270`). With the editor focused this view is a
    /// sibling subtree, not an ancestor, so a handler registered here would
    /// never be reached.
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
        let (parent, at, depth) = self.insertion_point(cx);

        let (field, subscriptions) = Self::open_field("", cx);
        window.focus(&field.focus_handle(cx), cx);
        self.editing = Some(EditState {
            target: None,
            parent,
            kind,
            at,
            depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
        // See the field on `Sidebar`: a row outside the rendered range has no
        // dispatch node, so the field would take no keystrokes at all.
        self.scroll.scroll_to_item(at, ScrollStrategy::Center);
        cx.notify();
    }

    /// The field both gestures use, and the two subscriptions that end it.
    ///
    /// Shared because nothing about confirming or cancelling differs between
    /// creating and renaming — `EditState::target` is what tells them apart,
    /// and `confirm` reads it.
    fn open_field(initial: &str, cx: &mut Context<Self>) -> (Entity<NameField>, [Subscription; 2]) {
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
    fn rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        let Some(id) = self.selected else {
            return;
        };
        let Some(at) = self.index_of(id) else {
            return;
        };
        let row = self.visible[at].clone();

        let parent = self.parent_of(&row);
        let (field, subscriptions) = Self::open_field(&row.name, cx);
        window.focus(&field.focus_handle(cx), cx);
        self.editing = Some(EditState {
            target: Some(id),
            parent,
            kind: row.kind,
            at,
            depth: row.depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
        self.scroll.scroll_to_item(at, ScrollStrategy::Center);
        cx.notify();
    }

    /// Where a new entry goes, as `(parent, row index, indent)`.
    fn insertion_point(&mut self, cx: &mut Context<Self>) -> (Option<EntryId>, usize, usize) {
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

    /// The folder a row lives in: the nearest entry above it one level shallower.
    /// `None` for a top-level row, which is the vault root.
    fn parent_of(&self, row: &Row) -> Option<EntryId> {
        if row.depth == 0 {
            return None;
        }
        self.visible[..self.index_of(row.id)?]
            .iter()
            .rev()
            .find(|above| above.depth == row.depth - 1 && above.kind == Kind::Dir)
            .map(|above| above.id)
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
        // The one place the two gestures diverge, which is what makes the
        // `Option` worth more than a mode enum: everything above is shared.
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
    /// Answered here rather than in `vault`: it is a question about the live
    /// listing, not a property of a path. Compared on the file name a note
    /// would actually get, so `plan` and `plan.md` are one collision.
    /// `target` is the entry being renamed, which must not collide with
    /// itself: confirming a rename without changing the name is a no-op, not a
    /// duplicate. It also covers a case-only rename, where the old and new
    /// names differ but name one file.
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

    fn select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    /// Split out only so a test can read the dot: nothing gets text back out
    /// of the `Div`s `render_rows` returns.
    fn row_label(&self, row: &Row, cx: &App) -> String {
        let dot = if self.notes.read(cx).is_dirty(row.id, cx) {
            " •"
        } else {
            ""
        };
        format!("{}{dot}", row.name)
    }

    /// The field's row: the same box as any other, so the pitch does not move.
    ///
    /// `uniform_list` measures every row from item 0
    /// (`elements/uniform_list.rs:658-680`), so a field even a pixel taller
    /// would misplace every row below it — and the click tests that pin the
    /// pitch would start failing for a reason that has nothing to do with them.
    fn render_field(edit: &EditState) -> Div {
        let depth = f32::from(u8::try_from(edit.depth).unwrap_or(u8::MAX));
        div()
            .flex()
            .flex_row()
            .w_full()
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .pl(INDENT_BASE + INDENT_STEP * depth)
            .bg(theme::selected_row_background())
            .child(edit.field.clone())
    }

    /// One row per visible entry, plus the field's when a name is being typed.
    ///
    /// The field occupies an index of its own rather than replacing a row, so
    /// creating never hides the entry it was started from.
    fn row_at(&self, ix: usize) -> Option<Row> {
        let Some(edit) = self.editing.as_ref() else {
            return self.visible.get(ix).cloned();
        };
        // A rename replaces its row; a create inserts one. Otherwise renaming
        // would show the old name and the field for one entry at once.
        if edit.target.is_some() {
            return (ix != edit.at)
                .then(|| self.visible.get(ix).cloned())
                .flatten();
        }
        match ix.cmp(&edit.at) {
            std::cmp::Ordering::Less => self.visible.get(ix).cloned(),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => self.visible.get(ix - 1).cloned(),
        }
    }

    /// How many rows the list draws: the tree, plus one for a field that is
    /// inserting rather than replacing.
    /// `pub(crate)` for the rename test in `shell.rs`, which asserts the count
    /// does not move: a rename replaces its row rather than adding one.
    pub(crate) fn row_count(&self) -> usize {
        let inserting = self
            .editing
            .as_ref()
            .is_some_and(|edit| edit.target.is_none());
        self.visible.len() + usize::from(inserting)
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        range
            .map(|ix| {
                let Some(row) = self.row_at(ix) else {
                    // Only the field's index yields `None`, and only while one
                    // is open, so this cannot be reached without an `EditState`.
                    return self.editing.as_ref().map_or_else(div, Self::render_field);
                };
                self.render_row(&row, cx)
            })
            .collect()
    }

    /// `cx.processor` re-enters the sidebar entity in `render_rows`
    /// (`gpui/src/app/context.rs:260-267`); that is the only reason
    /// `cx.listener` is reachable from `uniform_list`'s `&mut App` closure.
    fn render_row(&self, row: &Row, cx: &mut Context<Self>) -> Div {
        let row = row.clone();
        {
            // Reading inside the draw is what repaints the dot; no
            // `cx.observe` needed and `Entity::cached` would break it
            // (`app.rs:2789-2823`, `view.rs:488`).
            let label = self.row_label(&row, cx);
            // `Vault::scan` stops at depth 64, so nothing saturates.
            let depth = f32::from(u8::try_from(row.depth).unwrap_or(u8::MAX));
            // `FluentBuilder::when` would read better, but gpui keeps that
            // trait private in this build (`gpui.rs:61`, `:163`).
            let background = if self.selected == Some(row.id) {
                theme::selected_row_background()
            } else {
                theme::sidebar_background()
            };
            div()
                .flex()
                .flex_row()
                // Load-bearing: an `auto` width is clickable only where the
                // text reaches (`elements/uniform_list.rs:508-510`).
                .w_full()
                // No explicit height: the row *is* its line box, so it
                // cannot clip its own text however the type is set.
                //
                // That identity is also the only way the type statement
                // below is under test. A fixed height would hold the row
                // pitch at 22 px whatever the text did inside it, and a
                // deleted `text_size`/`line_height` pair would silently
                // restore the clipping this file used to have — gpui's
                // default line box is `phi()` on a 16 px default, 25.89 px.
                // Tied to the pitch, that deletion moves every row, and
                // `a_click_far_down_the_list_lands_on_the_row_it_names`
                // fails. Pixels the eye would catch become pixels the suite
                // catches.
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .pl(INDENT_BASE + INDENT_STEP * depth)
                // Load-bearing: row height is measured from item 0 alone
                // (`elements/uniform_list.rs:658-680`), so a wrapping
                // filename overflows its row.
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(theme::sidebar_text())
                .bg(background)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |sidebar, _event, window, cx| {
                        // Focus first, so the tree's own bindings resolve
                        // from the very next keystroke. Straight to the
                        // handle rather than through `Focusable`, which is
                        // what `EditorView::on_mouse_down` already does.
                        window.focus(&sidebar.focus_handle, cx);
                        sidebar.activate(&row, cx);
                    }),
                )
                .child(label)
        }
    }
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // The context every binding below names. It has to sit on a node
            // that tracks a focus handle, or it is in no dispatch path and the
            // bindings are excluded from matching rather than merely outranked.
            .key_context("Sidebar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::rename))
            // A column, so the rejection message below has a line of its own.
            // Without it the list's `size_full` claims the whole height and
            // pushes the message off the bottom — which every test still
            // passes, because they read the state rather than the pixels. Only
            // the `sidebar-rejected` shot showed it.
            .flex()
            .flex_col()
            .size_full()
            .child(
                uniform_list(
                    "vault-tree",
                    self.row_count(),
                    cx.processor(Self::render_rows),
                )
                // `flex_1` for the height, so the list takes what the message
                // leaves rather than all of it. `w_full` because
                // `ListSizingBehavior::Auto` otherwise resolves to nothing and
                // paints nothing (`elements/list.rs:192-193`) — the width half
                // of the `size_full` this replaces.
                .flex_1()
                .w_full()
                .track_scroll(&self.scroll),
            )
            .children(
                self.editing
                    .as_ref()
                    .and_then(|edit| edit.rejected)
                    .map(|rejected| {
                        // Under the list rather than over the row: an overlay
                        // needs a deferred absolute layer, and the message is
                        // worth more than its placement is.
                        div()
                            .w_full()
                            .text_size(theme::UI_TEXT_SIZE)
                            .line_height(theme::UI_LINE_HEIGHT)
                            .pl(INDENT_BASE)
                            .text_color(theme::rejected_text())
                            .child(rejected.message())
                    }),
            )
    }
}

/// The vertical centre of row `ix`, for a test that clicks one.
///
/// Derived from the line height because the row has no height of its own: see
/// `render_rows`. A click computed here and landing elsewhere means the rendered
/// pitch has drifted from the stated one.
///
/// Lives beside the row height rather than in a test module, so `shell.rs` uses
/// the same definition: two suites clicking the sidebar must not each hard-code
/// the pitch and drift from it. The shot runner is the third caller, and the
/// reason the gate is not `#[cfg(test)]` alone — it clicks rows from a normal
/// build.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn row_center(ix: f32) -> Pixels {
    theme::UI_LINE_HEIGHT * (ix + 0.5)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Instant;

    use gpui::{
        Entity, Modifiers, Subscription, TestAppContext, VisualTestContext, point, prelude::*, px,
    };
    use inkra_text::ByteOffset;
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{Kind, NoteActivated, Row, Sidebar, row_center};
    use crate::vault::NoteStore;

    /// Stops short of the sidebar: `add_window` constructs the view itself.
    fn parts(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        let vault = cx.update(|cx| cx.new(|_| scanned));
        cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)))
    }

    /// The same, with a sidebar that is never drawn.
    fn open(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<Sidebar> {
        open_with_store(cx, fs).0
    }

    fn open_with_store(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (Entity<Sidebar>, Entity<NoteStore>) {
        let notes = parts(cx, fs);
        let sidebar = cx.update(|cx| cx.new(|cx| Sidebar::new(notes.clone(), cx)));
        (sidebar, notes)
    }

    /// A sidebar that *is* drawn, so a click has a layout to land in.
    fn drawn(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> (Entity<Sidebar>, VisualTestContext) {
        let notes = parts(cx, fs);
        let window = cx.add_window(move |_, cx| Sidebar::new(notes, cx));
        let sidebar = window.root(cx).expect("root view");
        (sidebar, VisualTestContext::from_window(window.into(), cx))
    }

    /// A walk landing while a name is typed can leave the field's row past the
    /// end of the tree. An unrendered row has no dispatch node, so the field
    /// would take no keystrokes at all — not even `escape`.
    #[gpui::test]
    fn a_field_survives_the_tree_shrinking_under_it(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        fs.insert_file("/vault/c.md", "gamma");
        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let last = row(&sidebar, cx, "c");
        sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
        sidebar.update(cx, Sidebar::rename_for_test);
        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));

        // Two notes vanish from outside, so the field's row is past the end.
        fs.remove(Path::new("/vault/b.md"));
        fs.remove(Path::new("/vault/c.md"));
        let vault = notes
            .read_with(cx, |notes, _| notes.vault().cloned())
            .expect("a vault");
        let children = futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault")))
            .expect("collect");
        vault.update(cx, |vault, cx| {
            vault.apply(children);
            cx.notify();
        });

        assert!(
            !sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
            "the renamed entry is gone, so the gesture has no target left"
        );
    }

    /// The same shrink, with a create: the field has no target to lose, so it
    /// stays open — and its row has to be one the list still draws.
    #[gpui::test]
    fn a_create_field_is_pulled_back_inside_the_tree(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let last = row(&sidebar, cx, "b");
        sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
        sidebar.update(cx, Sidebar::create_for_test);

        fs.remove(Path::new("/vault/b.md"));
        let vault = notes
            .read_with(cx, |notes, _| notes.vault().cloned())
            .expect("a vault");
        let children = futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault")))
            .expect("collect");
        vault.update(cx, |vault, cx| {
            vault.apply(children);
            cx.notify();
        });

        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
        assert!(
            sidebar.read_with(cx, |sidebar, _| sidebar.field_row() < sidebar.row_count()),
            "the field's row has to be one the list draws"
        );
    }

    /// The conflict file's only route to the user, asserted where the user
    /// would see it: `visible`, not `Vault::entries`.
    ///
    /// `visible` is a cache rebuilt by `new` and `toggle` alone. A test that
    /// counts vault entries instead passes while the sidebar paints a stale
    /// row list, which is exactly what happened: the vault's `notify` had no
    /// subscriber, and the file this whole feature exists to surface was
    /// invisible until the next launch.
    #[gpui::test]
    fn a_file_appearing_in_the_vault_reaches_a_row(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        assert_eq!(sidebar.read_with(cx, |sidebar, _| sidebar.visible.len()), 1);

        // What a conflict leaves behind, and then the rescan that follows it.
        fs.insert_file("/vault/a (conflict 1970-01-01 00-00-01Z).md", "theirs");
        let vault = notes
            .read_with(cx, |notes, _| notes.vault().cloned())
            .expect("a vault");
        let children = futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault")))
            .expect("collect");
        vault.update(cx, |vault, cx| {
            vault.apply(children);
            cx.notify();
        });

        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.visible.len()),
            2,
            "the sidebar must observe the vault, not only its own toggles"
        );
    }

    /// Every `NoteActivated` the sidebar emits, in order.
    fn watch(
        sidebar: &Entity<Sidebar>,
        cx: &mut TestAppContext,
    ) -> (Rc<RefCell<Vec<NoteActivated>>>, Subscription) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let guard = cx.update(|cx| {
            let seen = Rc::clone(&seen);
            cx.subscribe(sidebar, move |_, event: &NoteActivated, _| {
                seen.borrow_mut().push(*event);
            })
        });
        (seen, guard)
    }

    /// Trailing `/` on directories: the display name is the file stem, so
    /// `Projects` and `Projects.md` are otherwise indistinguishable.
    fn labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
        sidebar.update(cx, |sidebar, _| sidebar.visible.iter().map(label).collect())
    }

    fn label(row: &Row) -> String {
        match row.kind {
            Kind::Dir => format!("{}/", row.name),
            Kind::Note => row.name.clone(),
        }
    }

    /// Dot included, unlike `labels`, which rebuilds the name itself.
    fn row_labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
        sidebar.update(cx, |sidebar, cx| {
            let rows = sidebar.visible.clone();
            rows.iter().map(|row| sidebar.row_label(row, cx)).collect()
        })
    }

    fn row(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, wanted: &str) -> Row {
        sidebar.update(cx, |sidebar, _| {
            sidebar
                .visible
                .iter()
                .find(|row| label(row) == wanted)
                .cloned()
                .expect("the row is visible")
        })
    }

    /// A byte-prefix skip would swallow `Projects.md` with the folder.
    #[gpui::test]
    fn collapsing_a_folder_hides_its_children_and_nothing_else(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "p");
        fs.insert_file("/vault/Projects.md", "n");
        fs.insert_file("/vault/a.md", "a");

        let sidebar = open(cx, fs as Arc<dyn Fs>);

        // Folders start collapsed.
        assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);

        let projects = row(&sidebar, cx, "Projects/");
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));
        assert_eq!(labels(&sidebar, cx), ["Projects/", "plan", "Projects", "a"]);

        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));
        assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);
    }

    /// Opening `Projects` must not open `Projects/deep` with it.
    #[gpui::test]
    fn expanding_a_folder_leaves_its_nested_folders_collapsed(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/deep/x.md", "x");
        fs.insert_file("/vault/Projects/plan.md", "p");

        let sidebar = open(cx, fs as Arc<dyn Fs>);
        assert_eq!(labels(&sidebar, cx), ["Projects/"]);

        let projects = row(&sidebar, cx, "Projects/");
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));

        assert_eq!(
            labels(&sidebar, cx),
            ["Projects/", "deep/", "plan"],
            "`x` lives inside `deep`, which nobody expanded"
        );
    }

    /// Nothing in the type system stops every row's `on_mouse_down` seeing the
    /// same press (`elements/uniform_list.rs:508-510`). The toggle side of
    /// that; the note side is `a_click_reports_the_row_it_landed_on`.
    #[gpui::test]
    fn a_press_on_one_row_does_not_reach_the_others(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "plan");
        fs.insert_file("/vault/Projects.md", "folder note");
        fs.insert_file("/vault/a.md", "alpha");

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);

        // The first row is the folder `Projects`.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();

        assert_eq!(
            labels(&sidebar, cx),
            ["Projects/", "plan", "Projects", "a"],
            "exactly one row responded"
        );
    }

    /// Subscribes to the `Sidebar` itself, so it proves nothing about
    /// `AppShell`'s wiring — that is `shell.rs`'s job.
    ///
    /// A real mouse event, not a call to `activate`: only the layout decides
    /// whether a click lands. Discriminates `.w_full()` — headless text is
    /// measured at 0.6 × `font_size` per glyph
    /// (`vendor/zed-reference/crates/gpui/src/platform.rs:1288-1296`), so an
    /// `auto` row around the stem `a` ends at 17.6px and x=20 misses.
    ///
    /// Does *not* cover `.size_full()`: a window root resolves its percentage
    /// against the test display's definite bounds, never the real `SIDEBAR_WIDTH` parent
    /// in `shell.rs`. The folder keeps a sibling note so that clicking it
    /// proves no *other* row reported either.
    #[gpui::test]
    fn a_click_reports_the_row_it_landed_on(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "plan");
        fs.insert_file("/vault/a.md", "alpha");

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
        let (seen, _guard) = watch(&sidebar, cx);

        // Row 0 is the folder `Projects`.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();
        assert!(
            seen.borrow().is_empty(),
            "a folder reports no note, its own or a sibling's"
        );

        // Expanded, so `a` is now row 2: `Projects/`, `plan`, `a`.
        let a = row(&sidebar, cx, "a");
        visual.simulate_click(point(px(20.), row_center(2.)), Modifiers::none());
        visual.run_until_parked();
        assert_eq!(*seen.borrow(), [NoteActivated(a.id)]);
    }

    /// The row pitch, asserted where a drift has somewhere to show.
    ///
    /// `render_rows` states the sidebar's type and lets each row take its line
    /// box as its height. Nothing but that identity holds the two together, so
    /// this is the test the type statement lives or dies by: a click computed
    /// from `UI_LINE_HEIGHT` has to land on the row that constant names.
    ///
    /// Thirty rows is what makes it bite, and the depth is the whole design.
    /// The two pitches below are **measured**, not derived — deleting the pair
    /// and probing where row 1 begins gives 26.0 px, and deleting `line_height`
    /// alone gives 22.5 px. (The second is not `phi()` times 14, which would be
    /// 22.65; gpui does not render the fraction it computes.) Against the stated
    /// 22 px, 26.0 first misplaces a click at row 3, and 22.5 — half a pixel of
    /// drift per row — not until row 23. Clicking row 29 catches both with six
    /// rows in hand. Twenty rows caught only the first.
    ///
    /// What it pins is `line_height`, which is the row's height and therefore
    /// the only thing that can make a line box outgrow its row. Deleting
    /// `text_size` alone leaves this green, and should: 16 px inside a stated
    /// 22 px box still fits, so that is a typographic regression and not the
    /// clipping defect. Nothing here catches it, and nothing here claims to.
    ///
    /// The alternative was a fixed `.h()` on the row, which is what this file
    /// used to have. It holds the pitch at 22 px whatever the text does inside
    /// it — so the clipping the type statement exists to prevent comes back
    /// silently, and every test here still passes. That is the state this test
    /// exists to leave behind.
    #[gpui::test]
    fn a_click_far_down_the_list_lands_on_the_row_it_names(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        // Two digits, so the comparator's lexical order is the numeric one:
        // `note-9` would sort between `note-19` and `note-2`.
        for ix in 0..30 {
            fs.insert_file(format!("/vault/note-{ix:02}.md"), "text");
        }

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
        let (seen, _guard) = watch(&sidebar, cx);

        let last = row(&sidebar, cx, "note-29");
        visual.simulate_click(point(px(20.), row_center(29.)), Modifiers::none());
        visual.run_until_parked();

        assert_eq!(
            *seen.borrow(),
            [NoteActivated(last.id)],
            "the rendered pitch has drifted from the stated line height"
        );
    }

    /// Asserts the label, not `is_dirty`: the predicate passes with the dot
    /// deleted. One note edited, so a marker on every row also fails.
    #[gpui::test]
    async fn an_edited_note_is_marked_in_the_tree_until_it_is_saved(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");

        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let b = row(&sidebar, cx, "b");

        let opened = notes.update(cx, |notes, cx| notes.open_note(b.id, cx));
        let buffer = opened.await.expect("`b` opens");
        notes.update(cx, |notes, cx| notes.set_current(b.id, cx));
        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "opening a note changes nothing about it"
        );

        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        });

        assert_eq!(row_labels(&sidebar, cx), ["a", "b •"]);

        notes.update(cx, NoteStore::save_current);
        cx.run_until_parked();

        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "the write caught the file up, so the note is no longer modified"
        );
    }
}
