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

actions!(sidebar, [Trash, SelectPrev, SelectNext, Rename]);

actions!(vault_tree, [NewNote, NewFolder]);

const INDENT_BASE: Pixels = px(8.);
const INDENT_STEP: Pixels = px(12.);

/// A struct, not an enum: `EventEmitter<E>` needs one type per event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteActivated(pub(crate) EntryId);

impl EventEmitter<NoteActivated> for Sidebar {}

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    expanded: HashSet<EntryId>,
    /// Not `NoteStore::current`: a row can be selected unopened.
    selected: Option<EntryId>,
    focus_handle: FocusHandle,
    visible: Vec<Row>,
    editing: Option<EditState>,
    /// Load-bearing: `uniform_list` only renders the visible range, and an
    /// unrendered focus handle falls back to the root, swallowing keystrokes.
    scroll: UniformListScrollHandle,
    /// Never read; dropping a `Subscription` cancels it.
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
            .key_context("Sidebar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::rename))
            .flex()
            .flex_col()
            .size_full()
            .child(
                uniform_list(
                    "vault-tree",
                    self.row_count(),
                    cx.processor(Self::render_rows),
                )
                // `w_full`: `ListSizingBehavior::Auto` otherwise paints nothing (`elements/list.rs:192-193`).
                .flex_1()
                .w_full()
                .track_scroll(&self.scroll),
            )
            .children(
                self.editing
                    .as_ref()
                    .and_then(|edit| edit.rejected)
                    .map(|rejected| {
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
