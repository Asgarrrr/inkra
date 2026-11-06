//! The shell's layout: the title row over the docks and the note column, and the picker over them.

use gpui::{
    App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use inkra_layout::{GroupId, Side};

use super::dock_drag::SHELL_GROUP;
use super::group::selector;
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

impl AppShell {
    /// Centre group `k`'s column: its nav row, its banner, the notice if it is the last active, and its note.
    fn render_note_column(
        &self,
        k: usize,
        group: GroupId,
        flex: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = theme::current(cx).colors;
        div()
            .id(("group-column", group.raw()))
            .debug_selector(move || selector("note-column", k))
            .flex_basis(px(0.))
            .flex_grow(flex)
            // `min_w_0`: else the note's content width would widen the column past the window.
            .min_w_0()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .capture_any_mouse_down(cx.listener(move |shell, _, window, cx| {
                shell.activate_group(group, window, cx);
            }))
            .child(self.render_nav_row(k, group, cx))
            .children(self.render_gone_banner(k, group, cx))
            .children(
                (group == self.layout.last_center())
                    .then(|| self.render_notice(cx))
                    .flatten(),
            )
            // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
            .child(div().flex_1().min_h_0().child(match self.editor_of(group) {
                Some(editor) => editor.clone().into_any_element(),
                None => self.render_empty(cx).into_any_element(),
            }))
            // Drawn over the column, not as its border: a border would widen it past its strip.
            .when(k > 0, |column| {
                column.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(theme::RULE_WIDTH)
                        .bg(colors.border_variant),
                )
            })
    }

    /// Under the title row: the left dock, the centre's columns and the right dock.
    fn render_columns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            // A definite width: else taffy measures the row's content, and a keystroke shapes the note twice.
            .w_full()
            // `min_h_0`: a flex child's minimum is its content, which would outgrow the window.
            .min_h_0()
            .children(self.render_dock(Side::Left, cx))
            // In the row itself, not a wrapper: a nested row has taffy measure the note's content.
            .children(
                self.layout
                    .center()
                    .iter()
                    .zip(self.layout.flexes())
                    .enumerate()
                    .map(|(k, (&group, &flex))| self.render_note_column(k, group, flex, cx))
                    .collect::<Vec<_>>(),
            )
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
            .on_action(cx.listener(Self::toggle_left_dock))
            .on_action(cx.listener(Self::toggle_right_dock))
            .on_action(cx.listener(Self::move_files_to_other_dock))
            .on_action(cx.listener(Self::move_backlinks_to_other_dock))
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
