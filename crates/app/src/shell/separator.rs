//! The line between two members of a split: a drag resizes them, a double-click equalises the split (splits spec, P2).

use gpui::{
    App, AppContext, Bounds, Context, Div, DragMoveEvent, ElementId, Empty, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Render, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use inkra_layout::{Axis, Member, SplitPath, Surface};

use super::AppShell;
use super::dock_chrome::name;
use super::group::selector;
use super::members::Slot;
use crate::theme;

const SEPARATOR_HITBOX: Pixels = px(4.);

#[derive(Clone)]
pub(super) struct DraggedSeparator {
    path: SplitPath,
    ix: usize,
    axis: Axis,
}

impl Render for DraggedSeparator {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl AppShell {
    /// Over the start of the member at `path`, whose first group is `d`th depth-first; `None` for a first member.
    pub(super) fn render_separator(
        path: &SplitPath,
        slot: Slot,
        d: usize,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let (&ix, steps) = path.steps.split_last()?;
        if ix == 0 {
            return None;
        }
        let dragged = DraggedSeparator {
            path: SplitPath {
                surface: path.surface,
                steps: steps.to_vec(),
            },
            ix,
            axis: slot.axis,
        };
        let id = match path.surface {
            Surface::Center => selector("separator", d),
            Surface::Dock(side) => selector(format_args!("dock-{}-separator", name(side)), d),
        };
        let colors = theme::current(cx).colors;
        let inset = (SEPARATOR_HITBOX - theme::RULE_WIDTH) / 2.;
        let line = div().absolute().bg(colors.border_variant);
        let separator = div()
            .id(ElementId::Name(id.clone().into()))
            .debug_selector(move || id)
            .absolute()
            .occlude();
        let separator = match slot.axis {
            Axis::Horizontal => separator
                .top_0()
                .bottom_0()
                .left(-inset)
                .w(SEPARATOR_HITBOX)
                .cursor_col_resize()
                .child(line.top_0().bottom_0().left(inset).w(theme::RULE_WIDTH)),
            Axis::Vertical => separator
                .left_0()
                .right_0()
                .top(-inset)
                .h(SEPARATOR_HITBOX)
                .cursor_row_resize()
                .child(line.left_0().right_0().top(inset).h(theme::RULE_WIDTH)),
        };
        let split = dragged.path.clone();
        Some(
            separator
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                        // Keeps the editor's focus: the root would take it on the press (`div.rs:2765-2780`).
                        window.prevent_default();
                        if event.click_count >= 2 {
                            shell.change_layout(window, cx, |shell, _, _| {
                                shell.layout.equalize(&split);
                            });
                        }
                    }),
                )
                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone())),
        )
    }

    /// On the container of the split at `path`; `lead` and `trail` are its bounds' ends that hold no member.
    pub(super) fn on_separator_drag(
        path: SplitPath,
        (lead, trail): (Pixels, Pixels),
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragMoveEvent<DraggedSeparator>, &mut Window, &mut App) + 'static {
        cx.listener(
            move |shell, event: &DragMoveEvent<DraggedSeparator>, window, cx| {
                let dragged = event.drag(cx);
                if dragged.path != path {
                    return;
                }
                let (ix, axis) = (dragged.ix, dragged.axis);
                // A tree change mid-drag can take the separator away.
                let Some(flexes) = shell
                    .layout
                    .member_at(&path)
                    .and_then(|member| match member {
                        Member::Split { flexes, .. } if ix < flexes.len() => Some(flexes),
                        _ => None,
                    })
                else {
                    cx.stop_active_drag(window);
                    return;
                };
                let (origin, extent, cursor) = along(axis, event.bounds, event.event.position);
                let size = f32::from(extent - lead - trail);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a count far below 2^24 converts to f32 exactly"
                )]
                let unit = size / flexes.len() as f32;
                let boundary = origin + lead + px(flexes[..ix].iter().sum::<f32>() * unit);
                let delta = f32::from(cursor - boundary);
                let mins = shell.min_sizes(window);
                let path = path.clone();
                shell.change_layout(window, cx, move |shell, _, _| {
                    shell
                        .layout
                        .resize(&path, ix, delta, size, |group, axis| mins[&(group, axis)]);
                });
            },
        )
    }
}

/// `bounds`' start and length along `axis`, and `position`'s coordinate on it.
fn along(
    axis: Axis,
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> (Pixels, Pixels, Pixels) {
    match axis {
        Axis::Horizontal => (bounds.left(), bounds.size.width, position.x),
        Axis::Vertical => (bounds.top(), bounds.size.height, position.y),
    }
}
