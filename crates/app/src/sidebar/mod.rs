//! The vault tree: a linear walk over path-ordered entries, never a per-frame
//! tree rebuild. `Vault::entries` yields a directory's descendants in a
//! contiguous run after it, so collapsing one is a forward skip.

use std::collections::HashSet;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Styled, Subscription, UniformListScrollHandle, Window, actions,
    div, px, uniform_list,
};
use inkra_vault::{EntryId, Kind};

use crate::theme;
use crate::vault::NoteStore;

mod edit;
mod render;
#[cfg(test)]
mod tests;
mod tree;

use edit::EditState;
#[cfg(any(test, feature = "shot"))]
pub(crate) use render::row_center;

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
