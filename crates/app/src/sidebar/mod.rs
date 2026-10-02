use std::collections::HashSet;
use std::path::Path;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Subscription,
    UniformListScrollHandle, Window, actions, div, uniform_list,
};
use inkra_vault::{EntryId, Kind, Vault};

use crate::open::NoteActivated;
use crate::theme;
use crate::ui::{PanelHeader, UiText};
use crate::vault::NoteStore;

mod edit;
mod render;
#[cfg(test)]
mod render_tests;
mod rows;
#[cfg(test)]
mod rows_tests;
#[cfg(test)]
mod tests;
mod tree;
mod visible;
#[cfg(test)]
mod visible_tests;

use edit::EditState;
#[cfg(any(test, feature = "shot"))]
pub(crate) use render::row_center;
use rows::Rows;

actions!(sidebar, [Trash, SelectPrev, SelectNext, Rename]);

actions!(vault_tree, [NewNote, NewFolder]);

impl EventEmitter<NoteActivated> for Sidebar {}

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    vault: Entity<Vault>,
    expanded: HashSet<EntryId>,
    /// Not `NoteStore::current`: a row can be selected unopened.
    selected: Option<EntryId>,
    /// Bumped by every tree write of `selected` (`select`), never by `follow`.
    cursor: Cursor,
    focus_handle: FocusHandle,
    /// The vault root's folder name, for the header and the tree's label.
    title: SharedString,
    rows: Rows<EditState>,
    /// Load-bearing: `uniform_list` only renders the visible range, and an
    /// unrendered focus handle falls back to the root, swallowing keystrokes.
    scroll: UniformListScrollHandle,
    /// Never read; dropping a `Subscription` cancels it.
    _vault_changed: Subscription,
}

/// The tree's write generation when an open began; `follow` moves `selected` only if it still holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Cursor(u64);

#[derive(Clone)]
struct Row {
    id: EntryId,
    depth: usize,
    kind: Kind,
    name: String,
}

impl Sidebar {
    pub(crate) fn new(
        notes: Entity<NoteStore>,
        vault: Entity<Vault>,
        cx: &mut Context<Self>,
    ) -> Self {
        let watched = cx.observe(&vault, |this, _, cx| this.rebuild(cx));
        let title = title_of(vault.read(cx).root());
        let mut sidebar = Self {
            notes,
            vault,
            expanded: HashSet::new(),
            selected: None,
            cursor: Cursor::default(),
            focus_handle: cx.focus_handle(),
            title,
            rows: Rows::default(),
            scroll: UniformListScrollHandle::default(),
            _vault_changed: watched,
        };
        sidebar.rebuild(cx);
        sidebar
    }
}

/// The root's folder name; the whole path for a root that has none, such as `/`.
fn title_of(root: &Path) -> SharedString {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
        .into()
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sidebar")
            .role(Role::Tree)
            .aria_label(self.title.clone())
            .key_context("Sidebar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::rename))
            .flex()
            .flex_col()
            .size_full()
            .child(PanelHeader::new("panel-header-sidebar", self.title.clone()))
            .child(
                uniform_list(
                    "vault-tree",
                    self.item_count(),
                    cx.processor(Self::render_rows),
                )
                // `w_full`: `ListSizingBehavior::Auto` otherwise paints nothing (`elements/list.rs:192-193`).
                .flex_1()
                .w_full()
                .track_scroll(&self.scroll),
            )
            .children(
                self.rows
                    .edit()
                    .and_then(|edit| edit.rejected)
                    .map(|rejected| {
                        div()
                            .w_full()
                            .ui_text()
                            .pl(theme::ROW_INSET)
                            .text_color(theme::rejected_text())
                            .child(rejected.message())
                    }),
            )
    }
}
