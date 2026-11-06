//! The shell's layout: the title row over the docks and the note column, and the picker over them.

use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Render, Styled, Window, div, prelude::FluentBuilder, px,
};
use inkra_layout::{Axis, GroupId, Member, Side};

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

/// Where a centre member sits: its parent's axis, its index there, and its flex.
#[derive(Clone, Copy)]
struct Slot {
    axis: Axis,
    ix: usize,
    flex: f32,
}

impl Slot {
    /// The root, as the main row's one centre member.
    const ROOT: Self = Self {
        axis: Axis::Horizontal,
        ix: 0,
        flex: 1.0,
    };

    /// Basis 0, so the flexes alone share the parent's room; `min_*_0`, else content would widen it.
    fn size<E: Styled>(self, element: E) -> E {
        let element = element.flex_basis(px(0.)).flex_grow(self.flex).relative();
        match self.axis {
            Axis::Horizontal => element.min_w_0().h_full(),
            Axis::Vertical => element.min_h_0().w_full(),
        }
    }

    /// Drawn over the member, not as its border: a border would widen it past its strip.
    fn rule(self, cx: &App) -> Option<Div> {
        let colors = theme::current(cx).colors;
        (self.ix > 0).then(|| {
            let rule = div().absolute().left_0().top_0().bg(colors.border_variant);
            match self.axis {
                Axis::Horizontal => rule.bottom_0().w(theme::RULE_WIDTH),
                Axis::Vertical => rule.right_0().h(theme::RULE_WIDTH),
            }
        })
    }
}

impl AppShell {
    /// A lower group's strip, as the title row holds only the top groups' (splits spec, D4).
    fn render_group_header(&self, k: usize, group: GroupId, cx: &mut Context<Self>) -> Div {
        let colors = theme::current(cx).colors;
        h_flex()
            .debug_selector(move || selector("group-header", k))
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
            .child(self.render_tab_bar(k, group, cx))
            .child(Self::render_new_tab(k, cx))
    }

    /// Centre group `k`'s column: its header if it is lower, its nav row, its banner, the notice if
    /// it is the last active, and its note.
    fn render_note_column(
        &self,
        k: usize,
        group: GroupId,
        slot: Slot,
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
        // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
        .child(div().flex_1().min_h_0().child(match self.editor_of(group) {
            Some(editor) => editor.clone().into_any_element(),
            None => self.render_empty(cx).into_any_element(),
        }))
        .children(slot.rule(cx))
    }

    /// `k` counts the groups drawn so far, depth-first.
    fn render_member(
        &self,
        member: &Member,
        slot: Slot,
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match member {
            Member::Group(group) => {
                let lower = !top.contains(group);
                let column = self.render_note_column(*k, *group, slot, lower, cx);
                *k += 1;
                column.into_any_element()
            }
            Member::Split {
                axis,
                children,
                flexes,
            } => {
                let children = self.render_split(*axis, children, flexes, top, k, cx);
                slot.size(div().flex().when(*axis == Axis::Vertical, Styled::flex_col))
                    .children(children)
                    .children(slot.rule(cx))
                    .into_any_element()
            }
        }
    }

    fn render_split(
        &self,
        axis: Axis,
        children: &[Member],
        flexes: &[f32],
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        children
            .iter()
            .zip(flexes)
            .enumerate()
            .map(|(ix, (child, &flex))| {
                self.render_member(child, Slot { axis, ix, flex }, top, k, cx)
            })
            .collect()
    }

    /// Under the title row: the left dock, the centre and the right dock.
    fn render_columns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let top = self.layout.top_groups();
        let mut k = 0;
        // In the row itself, not a wrapper: a nested row has taffy measure the note's content.
        let center = match self.layout.center() {
            Member::Split {
                axis: Axis::Horizontal,
                children,
                flexes,
            } => self.render_split(Axis::Horizontal, children, flexes, &top, &mut k, cx),
            member => vec![self.render_member(member, Slot::ROOT, &top, &mut k, cx)],
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
