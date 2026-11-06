//! The shell's layout: the title row over the docks and the note column, and the picker over them.

use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Render, Styled, Window, div, prelude::FluentBuilder, px, relative,
};
use inkra_layout::{Axis, GroupId, Member, Side, SplitDirection, SplitPath, Surface};

use super::dock_drag::{DraggedView, SHELL_GROUP};
use super::group::selector;
use super::tab_bar::DraggedTab;
use super::{AppShell, GoBack, GoForward};
use crate::theme;
use crate::ui::{Color, IconButton, IconName, h_flex, keep_focus};

impl AppShell {
    /// None while the restore is pending, so the launch does not flash the empty state before the note.
    pub(super) fn empty_lines(&self) -> &'static [&'static str] {
        if self.panes.is_none() {
            &[
                "No vault is open.",
                "Restart with a folder path to open one.",
            ]
        } else if self.restore_pending() {
            &[]
        } else {
            &["Select a note."]
        }
    }

    fn render_empty(&self, cx: &App) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let lines = self.empty_lines();
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // Without this the window default shows through.
            .bg(colors.editor_background)
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(Color::Placeholder.rgba(theme::current(cx)))
            .children(lines.iter().map(|line| div().child(*line)))
    }

    /// Over the note `group` shows while it is gone from disk (watcher spec, F9).
    fn render_gone_banner(&self, k: usize, group: GroupId, cx: &App) -> Option<impl IntoElement> {
        let showing = self.group(group).shown.bound()?.note;
        let colors = theme::current(cx).colors;
        self.notes.read(cx).is_gone(showing).then(|| {
            div()
                .debug_selector(move || selector("gone-banner", k))
                .flex_none()
                .px_4()
                .bg(colors.warning_background)
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .text_color(Color::Warning.rgba(theme::current(cx)))
                .child("Deleted from disk. Typing restores it.")
        })
    }

    /// Atop the note column, out of the editor's scroll; each arrow is lit when its walk would arrive (D4).
    fn render_nav_row(&self, k: usize, group: GroupId, cx: &mut Context<Self>) -> impl IntoElement {
        let group = self.group(group);
        let showing = group.shown.note();
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(move || selector("nav-row", k))
            .flex_none()
            .h(theme::NAV_ROW_HEIGHT)
            .pl_3()
            .bg(colors.editor_background)
            // A press on the strip beside the arrows leaves the editor focused.
            .on_mouse_down(MouseButton::Left, keep_focus)
            .child(
                IconButton::new("nav-back", IconName::ArrowLeft, "Go back")
                    .disabled(!group.tabs.can_go_back(showing))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.go_back(&GoBack, window, cx);
                    })),
            )
            .child(
                IconButton::new("nav-forward", IconName::ArrowRight, "Go forward")
                    .disabled(!group.tabs.can_go_forward(showing))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.go_forward(&GoForward, window, cx);
                    })),
            )
    }
}

/// Where a member sits: its parent's axis and its flex.
#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub(super) axis: Axis,
    flex: f32,
}

impl Slot {
    /// The root, as its row's one member.
    pub(super) const ROOT: Self = Self {
        axis: Axis::Horizontal,
        flex: 1.0,
    };

    /// Basis 0, so the flexes alone share the parent's room; `min_*_0`, else content would widen it.
    pub(super) fn size<E: Styled>(self, element: E) -> E {
        let element = element.flex_basis(px(0.)).flex_grow(self.flex).relative();
        match self.axis {
            Axis::Horizontal => element.min_w_0().h_full(),
            Axis::Vertical => element.min_h_0().w_full(),
        }
    }
}

/// A lower group's header row, as the title row holds only the top groups' strips (splits spec, D4).
pub(super) fn lower_header(cx: &App) -> Div {
    let colors = theme::current(cx).colors;
    h_flex()
        .flex_none()
        .relative()
        .h(theme::GROUP_HEADER_HEIGHT)
        .bg(colors.background)
        .on_mouse_down(MouseButton::Left, keep_focus)
        // Under the tabs, as the title row's; the active tab, painted later, breaks it.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .border_b(theme::RULE_WIDTH)
                .border_color(colors.border_variant),
        )
}

impl AppShell {
    fn render_group_header(&self, k: usize, group: GroupId, cx: &mut Context<Self>) -> Div {
        lower_header(cx)
            .debug_selector(move || selector("group-header", k))
            .child(self.render_tab_bar(k, group, cx))
            .child(Self::render_new_tab(k, cx))
            .on_drop(Self::view_drop(group, cx))
    }

    /// A view dropped on a centre group's strip lands as its last tab.
    pub(super) fn view_drop(
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&DraggedView, &mut Window, &mut App) + 'static {
        cx.listener(move |shell, dragged: &DraggedView, window, cx| {
            shell.drop_view_at_end(dragged.view, group, window, cx);
        })
    }

    /// The view and its header, else the note or the empty state. A view or a tab dropped near an
    /// edge splits the group there; elsewhere, one from another group lands as the last tab.
    fn render_group_body(&self, k: usize, group: GroupId, cx: &mut Context<Self>) -> Div {
        let colors = theme::current(cx).colors;
        let view = self.group(group).shown.view();
        // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
        let body = div()
            .flex_1()
            .min_h_0()
            .relative()
            .on_drag_move(Self::track_view_drop(group, cx))
            .on_drag_move(Self::track_tab_drop(group, cx))
            .drag_over::<DraggedView>(move |style, _, _, _| style.bg(colors.accent_background))
            .on_drop(Self::view_body_drop(group, cx))
            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                if dragged.group == group {
                    style
                } else {
                    style.bg(colors.accent_background)
                }
            })
            .on_drop(Self::tab_body_drop(group, cx));
        let overlay = self.render_drop_overlay(group, selector("drop-overlay", k), cx);
        match view {
            Some(view) => body
                .flex()
                .flex_col()
                .child(self.render_view_header(view, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .children(self.render_view_body(view)),
                ),
            None => body.child(match self.editor_of(group) {
                Some(editor) => editor.clone().into_any_element(),
                None => self.render_empty(cx).into_any_element(),
            }),
        }
        .children(overlay)
    }

    /// The half of `group`'s body a drop would split off; drawn only while a drag is over it.
    pub(super) fn render_drop_overlay(
        &self,
        group: GroupId,
        selector: String,
        cx: &App,
    ) -> Option<Div> {
        let direction = self.group(group).drop_split?;
        let colors = theme::current(cx).colors;
        let overlay = div()
            .debug_selector(move || selector)
            .absolute()
            .bg(colors.accent_background)
            .invisible()
            .drag_over::<DraggedTab>(|style, _, _, _| style.visible())
            .drag_over::<DraggedView>(|style, _, _, _| style.visible());
        Some(match direction {
            SplitDirection::Up => overlay.top_0().left_0().right_0().h(relative(0.5)),
            SplitDirection::Down => overlay.bottom_0().left_0().right_0().h(relative(0.5)),
            SplitDirection::Left => overlay.top_0().left_0().bottom_0().w(relative(0.5)),
            SplitDirection::Right => overlay.top_0().right_0().bottom_0().w(relative(0.5)),
        })
    }

    /// Centre group `k`'s column: its header if it is lower, its nav row, its banner, the notice if
    /// it is the last active, and its body.
    fn render_note_column(
        &self,
        k: usize,
        group: GroupId,
        (slot, path): (Slot, &SplitPath),
        lower: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        slot.size(
            div()
                .id(("group-column", group.raw()))
                .debug_selector(move || selector("note-column", k)),
        )
        .flex()
        .flex_col()
        .capture_any_mouse_down(cx.listener(move |shell, _, window, cx| {
            shell.activate_group(group, window, cx);
        }))
        .when(lower, |column| {
            column.child(self.render_group_header(k, group, cx))
        })
        .child(self.render_nav_row(k, group, cx))
        .children(self.render_gone_banner(k, group, cx))
        .children(
            (group == self.layout.last_center())
                .then(|| self.render_notice(cx))
                .flatten(),
        )
        .child(self.render_group_body(k, group, cx))
        .children(Self::render_separator(path, slot, k, cx))
    }

    /// `k` counts `path.surface`'s groups drawn so far, depth-first; `path` names `member`.
    pub(super) fn render_member(
        &self,
        member: &Member,
        slot: Slot,
        path: &SplitPath,
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match member {
            Member::Group(group) => {
                let lower = !top.contains(group);
                let column = match path.surface {
                    Surface::Center => self
                        .render_note_column(*k, *group, (slot, path), lower, cx)
                        .into_any_element(),
                    Surface::Dock(side) => self
                        .render_view_column(side, *k, *group, (slot, path), lower, cx)
                        .into_any_element(),
                };
                *k += 1;
                column
            }
            Member::Split {
                axis,
                children,
                flexes,
            } => {
                let separator = Self::render_separator(path, slot, *k, cx);
                let children =
                    self.render_split(*axis, children.iter().zip(flexes), path, top, k, cx);
                slot.size(div().flex().when(*axis == Axis::Vertical, Styled::flex_col))
                    .on_drag_move(Self::on_separator_drag(path.clone(), (px(0.), px(0.)), cx))
                    .children(children)
                    .children(separator)
                    .into_any_element()
            }
        }
    }

    /// `path` names the split.
    fn render_split<'a>(
        &self,
        axis: Axis,
        children: impl Iterator<Item = (&'a Member, &'a f32)>,
        path: &SplitPath,
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        children
            .enumerate()
            .map(|(ix, (child, &flex))| {
                let mut child_path = path.clone();
                child_path.steps.push(ix);
                self.render_member(child, Slot { axis, flex }, &child_path, top, k, cx)
            })
            .collect()
    }

    /// Under the title row: the left dock, the centre and the right dock.
    fn render_columns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let root = SplitPath {
            surface: Surface::Center,
            steps: Vec::new(),
        };
        let top = self.layout.top_groups(root.surface);
        let mut k = 0;
        let split_row = matches!(
            self.layout.center(),
            Member::Split {
                axis: Axis::Horizontal,
                ..
            }
        );
        // In the row itself, not a wrapper: a nested row has taffy measure the note's content.
        let center = match self.layout.center() {
            Member::Split {
                axis: Axis::Horizontal,
                children,
                flexes,
            } => self.render_split(
                Axis::Horizontal,
                children.iter().zip(flexes),
                &root,
                &top,
                &mut k,
                cx,
            ),
            member => vec![self.render_member(member, Slot::ROOT, &root, &top, &mut k, cx)],
        };
        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            // A definite width: else taffy measures the row's content, and a keystroke shapes the note twice.
            .w_full()
            // `min_h_0`: a flex child's minimum is its content, which would outgrow the window.
            .min_h_0()
            .when(split_row, |row| {
                row.on_drag_move(Self::on_separator_drag(root, self.dock_ends(), cx))
            })
            .children(self.render_dock(Side::Left, cx))
            .children(center)
            // Shown for an empty tab too, so a switch does not move the note column by the dock's width.
            .children(self.render_dock(Side::Right, cx))
            .children(self.render_drop_strip(Side::Left, cx))
            .children(self.render_drop_strip(Side::Right, cx))
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here, not on `EditorView`: a note row can hold focus too, and dispatch walks up to the root.
            .key_context("AppShell")
            .group(SHELL_GROUP)
            // Load-bearing: with no editor, focus lives on `self.focus_handle`; untracked, `cmd-s` is unreachable (gpui/src/window.rs:6244-6251).
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::search_vault))
            .on_action(cx.listener(Self::command_palette))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::close_active_tab))
            .on_action(cx.listener(Self::reopen_closed_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::activate_nth_tab))
            .on_action(cx.listener(Self::activate_last_tab))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(cx.listener(Self::split_right))
            .on_action(cx.listener(Self::split_down))
            .on_action(cx.listener(Self::split_left))
            .on_action(cx.listener(Self::split_up))
            .on_action(cx.listener(Self::activate_pane_left))
            .on_action(cx.listener(Self::activate_pane_right))
            .on_action(cx.listener(Self::activate_pane_up))
            .on_action(cx.listener(Self::activate_pane_down))
            .on_action(cx.listener(Self::toggle_left_dock))
            .on_action(cx.listener(Self::toggle_right_dock))
            .on_action(cx.listener(Self::move_files_to_other_dock))
            .on_action(cx.listener(Self::move_backlinks_to_other_dock))
            .on_action(cx.listener(Self::split_view))
            .on_action(cx.listener(Self::select_theme))
            .on_action(cx.listener(Self::quit))
            .on_mouse_move(cx.listener(Self::title_drag_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::title_drag_release))
            .flex()
            .flex_col()
            .size_full()
            .when_some(self.backdrop, Styled::bg)
            .child(self.render_tab_row(window, cx))
            .child(self.render_columns(cx))
            .children(self.render_modal(cx))
    }
}
