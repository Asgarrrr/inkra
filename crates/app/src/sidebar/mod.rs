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
        /// Reads `Sidebar::selected`, so it has a target only while the tree
        /// drives it.
        Trash,
        SelectPrev,
        SelectNext,
        /// Opens a field over the selected row, seeded with its current name.
        /// Reads `Sidebar::selected`, like `Trash`.
        Rename,
    ]
);

actions!(
    vault_tree,
    [
        /// Opens a field for a new note beside the tree's selection. Bound at
        /// the `AppShell` context so it fires with the editor focused;
        /// handled there, calling `Sidebar::start_creating`.
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
/// one plugs in by emitting this. A struct, not a one-variant enum:
/// `EventEmitter<E>` is generic over the event type (`gpui/src/gpui.rs:296`),
/// so a second signal needs a second type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteActivated(pub(crate) EntryId);

impl EventEmitter<NoteActivated> for Sidebar {}

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    /// Expansion is UI state, never vault state.
    expanded: HashSet<EntryId>,
    /// What a command acts on. Not the same as `NoteStore::current`: that
    /// names the note the editor shows, and a row can be selected unopened.
    selected: Option<EntryId>,
    /// Tracked so the `Sidebar` key context has a dispatch node; see `render`.
    focus_handle: FocusHandle,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
    /// A name being typed, and where its row sits. `None` the rest of the time.
    editing: Option<EditState>,
    /// Scrolls the field's row into view when one opens. Load-bearing:
    /// `uniform_list` renders only the visible range
    /// (`elements/uniform_list.rs:479-489`), and gpui does not blur a focused
    /// handle whose element stopped being drawn — dispatch falls back to the
    /// root node (`window.rs:6244-6251`), silently swallowing every keystroke.
    scroll: UniformListScrollHandle,
    /// Rebuilds `visible` when the vault's contents change. Never read;
    /// holding it is the point, since dropping a `Subscription` cancels it —
    /// without it, a file added to the vault while open never reaches a row.
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
            // Must sit on a node tracking a focus handle, or it's in no
            // dispatch path and the bindings below are excluded from matching.
            .key_context("Sidebar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::rename))
            // A column, so the rejection message below has a line of its own.
            // Without it the list's `size_full` claims the whole height and
            // pushes the message off the bottom.
            .flex()
            .flex_col()
            .size_full()
            .child(
                uniform_list(
                    "vault-tree",
                    self.row_count(),
                    cx.processor(Self::render_rows),
                )
                // `flex_1` so the list takes what the message leaves, not all
                // of it. `w_full` because `ListSizingBehavior::Auto` otherwise
                // resolves to nothing and paints nothing
                // (`elements/list.rs:192-193`).
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
                        // would need a deferred absolute layer.
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
