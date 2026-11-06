//! A group's column: the empty state, the note column's rows, and the body that takes drops (splits spec, P3).

use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, Window, div, prelude::FluentBuilder, relative,
};
use inkra_layout::{GroupId, SplitDirection, SplitPath};

use super::dock_drag::DraggedView;
use super::group::selector;
use super::members::Slot;
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
        let view = self.group(group).shown.view();
        let content = match view {
            Some(view) => vec![
                self.render_view_header(view, cx),
                div()
                    .flex_1()
                    .min_h_0()
                    .children(self.render_view_body(view))
                    .into_any_element(),
            ],
            None => vec![match self.editor_of(group) {
                Some(editor) => editor.clone().into_any_element(),
                None => self.render_empty(cx).into_any_element(),
            }],
        };
        let colors = theme::current(cx).colors;
        // Over the opaque centre, not in its place: a bare tint would show the window's blur.
        let tint = colors.editor_background.blend(colors.accent_background);
        let in_strip = self.drop_strip_hit();
        self.group_body(group, selector("drop-overlay", k), content, cx)
            .bg(colors.editor_background)
            .on_drag_move(Self::track_tab_drop(group, cx))
            .drag_over::<DraggedView>(move |style, _, window, _| {
                if in_strip(window.mouse_position(), window) {
                    style
                } else {
                    style.bg(tint)
                }
            })
            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                if dragged.group == group {
                    style
                } else {
                    style.bg(tint)
                }
            })
            .on_drop(Self::tab_body_drop(group, cx))
            .when(view.is_some(), |body| body.flex().flex_col())
    }

    /// The drop target under a group's rows: `content`, then the overlay of the split a drop would make.
    pub(super) fn group_body(
        &self,
        group: GroupId,
        overlay: String,
        content: impl IntoIterator<Item = AnyElement>,
        cx: &mut Context<Self>,
    ) -> Div {
        // `min_h_0`: a flex child's minimum is its content, which would outgrow the pane.
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .on_drag_move(Self::track_view_drop(group, cx))
            .on_drop(Self::view_body_drop(group, cx))
            .children(content)
            .children(self.render_drop_overlay(group, overlay, cx))
    }

    /// `base` names the column; then its `header` if it is lower, its `rows`, its `body` and its separator.
    pub(super) fn group_column<E: Styled + ParentElement>(
        base: E,
        (slot, path): (Slot, &SplitPath),
        k: usize,
        header: Option<Div>,
        rows: impl IntoIterator<Item = AnyElement>,
        body: Div,
        cx: &mut Context<Self>,
    ) -> E {
        slot.size(base)
            .flex()
            .flex_col()
            .children(header)
            .children(rows)
            .child(body)
            .children(Self::render_separator(path, slot, k, cx))
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
    pub(super) fn render_note_column(
        &self,
        k: usize,
        group: GroupId,
        (slot, path): (Slot, &SplitPath),
        lower: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let base = div()
            .id(("group-column", group.raw()))
            .debug_selector(move || selector("note-column", k))
            .capture_any_mouse_down(cx.listener(move |shell, _, window, cx| {
                shell.activate_group(group, window, cx);
            }));
        let header = lower.then(|| self.render_group_header(k, group, cx));
        let rows = [
            Some(self.render_nav_row(k, group, cx).into_any_element()),
            self.render_gone_banner(k, group, cx)
                .map(IntoElement::into_any_element),
            (group == self.layout.last_center())
                .then(|| self.render_notice(cx))
                .flatten()
                .map(IntoElement::into_any_element),
        ];
        let rows = rows.into_iter().flatten();
        let body = self.render_group_body(k, group, cx);
        Self::group_column(base, (slot, path), k, header, rows, body, cx)
    }
}
