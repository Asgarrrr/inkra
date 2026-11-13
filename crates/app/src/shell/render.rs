//! The shell's layout: the title row over the docks and the note column, and the picker over them.

use gpui::{
    Action, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled,
    Window, div, prelude::FluentBuilder,
};
use inkra_layout::{Axis, Member, Side, SplitPath, Surface};

use super::AppShell;
use super::dock_drag::SHELL_GROUP;
use super::members::Slot;
use crate::index::{IndexSync, RebuildIndex};
use crate::pane::CORE_PANES;

impl AppShell {
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
            .key_context(self.key_context())
            .group(SHELL_GROUP)
            // Load-bearing: with no editor, focus lives on `self.focus_handle`; untracked, `cmd-s` is unreachable (gpui/src/window.rs:6757-6764).
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::open_vault))
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
            .on_action(cx.listener(Self::go_to_stop))
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
            .map(|root| {
                CORE_PANES.iter().fold(root, |root, pane| {
                    let id = pane.id;
                    root.on_boxed_action(
                        &*(pane.move_action)(),
                        cx.listener(move |shell, _: &dyn Action, window, cx| {
                            shell.move_to_other_dock(id, window, cx);
                        }),
                    )
                })
            })
            .map(|root| {
                self.features.enabled().fold(root, |root, manifest| {
                    let id = manifest.id;
                    manifest.commands.iter().fold(root, |root, decl| {
                        root.on_boxed_action(
                            &*(decl.action)(),
                            cx.listener(move |shell, action: &dyn Action, window, cx| {
                                shell.run_command(id, action, window, cx);
                            }),
                        )
                    })
                })
            })
            .on_action(cx.listener(Self::split_view))
            .on_action(cx.listener(Self::select_theme))
            .on_action(cx.listener(|shell, _: &RebuildIndex, _, cx| {
                if let Some(panes) = &shell.panes {
                    panes.links.update(cx, IndexSync::rebuild);
                }
            }))
            .on_action(cx.listener(Self::quit))
            .on_mouse_move(cx.listener(Self::title_drag_move))
            .on_mouse_move(cx.listener(Self::press_move))
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
