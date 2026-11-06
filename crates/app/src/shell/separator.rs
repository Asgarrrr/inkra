//! The line between two members of a split: a drag resizes them, a double-click equalises the split (splits spec, P2).

use std::collections::HashMap;

use gpui::{
    App, AppContext, Bounds, Context, Div, DragMoveEvent, ElementId, Empty, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Render, Size, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px, size,
};
use inkra_layout::{Axis, GroupId, Member, Rect, Side, SplitPath, Surface};

use super::dock_chrome::{name, width};
use super::group::selector;
use super::render::Slot;
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::theme;

pub(super) const HORIZONTAL_MIN: f32 = 80.;
pub(super) const VERTICAL_MIN: f32 = 100.;
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
            .debug_selector(move || id.clone())
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
                #[allow(clippy::cast_precision_loss)]
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

    /// The main row's ends taken by the drawn docks.
    pub(super) fn dock_ends(&self) -> (Pixels, Pixels) {
        let drawn = |side, width| {
            if self.layout.is_drawn(side) {
                width
            } else {
                px(0.)
            }
        };
        (
            drawn(Side::Left, LEFT_DOCK_WIDTH),
            drawn(Side::Right, RIGHT_DOCK_WIDTH),
        )
    }

    /// Each group's least room along each axis: the floor, or the title-row chrome it draws if wider.
    fn min_sizes(&self, window: &Window) -> HashMap<(GroupId, Axis), f32> {
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        let leading = lights + theme::ICON_BUTTON_WIDTH;
        let trailing = theme::ICON_BUTTON_WIDTH + theme::ROW_INSET;
        let mut chrome: HashMap<GroupId, Pixels> = HashMap::new();
        let center = theme::TAB_CURVE * 2.
            + theme::TAB_MIN_WIDTH
            + theme::ICON_BUTTON_WIDTH
            + theme::TITLE_DRAG_MIN_WIDTH;
        for group in self.layout.center_groups() {
            chrome.insert(group, center);
        }
        for side in [Side::Left, Side::Right] {
            for group in self.layout.dock_groups(side) {
                #[allow(clippy::cast_precision_loss)]
                let views = self.group(group).views().count() as f32;
                chrome.insert(
                    group,
                    theme::ICON_BUTTON_WIDTH * views + theme::TITLE_DRAG_MIN_WIDTH,
                );
            }
        }
        let mut add = |group: Option<&GroupId>, room: Pixels| {
            if let Some(width) = group.and_then(|group| chrome.get_mut(group)) {
                *width += room;
            }
        };
        let center_top = self.layout.top_groups(Surface::Center);
        if self.layout.is_drawn(Side::Left) {
            add(
                self.layout.top_groups(Surface::Dock(Side::Left)).first(),
                leading,
            );
        } else {
            add(center_top.first(), leading);
        }
        if self.layout.is_drawn(Side::Right) {
            add(
                self.layout.top_groups(Surface::Dock(Side::Right)).last(),
                trailing,
            );
        } else {
            add(center_top.last(), trailing);
        }
        chrome
            .into_iter()
            .flat_map(|(group, width)| {
                [
                    (
                        (group, Axis::Horizontal),
                        f32::from(width).max(HORIZONTAL_MIN),
                    ),
                    ((group, Axis::Vertical), VERTICAL_MIN),
                ]
            })
            .collect()
    }

    /// Whether both halves of `group` split along `axis` would keep its minimum.
    pub(super) fn fits_split(&self, group: GroupId, axis: Axis, window: &Window) -> bool {
        let Some(rect) = self.rect_of(group) else {
            return false;
        };
        let room = self.surface_size(self.layout.surface_of(group), window);
        let (share, room) = match axis {
            Axis::Horizontal => (rect.width, room.width),
            Axis::Vertical => (rect.height, room.height),
        };
        f32::from(room) * share / 2. >= self.min_sizes(window)[&(group, axis)]
    }

    /// `group`'s share of its surface.
    pub(super) fn rect_of(&self, group: GroupId) -> Option<Rect> {
        self.layout
            .rects(self.layout.surface_of(group))
            .into_iter()
            .find_map(|(held, rect)| (held == group).then_some(rect))
    }

    /// The room `surface`'s unit square stands for, under the title row.
    pub(super) fn surface_size(&self, surface: Surface, window: &Window) -> Size<Pixels> {
        let viewport = window.viewport_size();
        let width = match surface {
            Surface::Center => {
                let (lead, trail) = self.dock_ends();
                viewport.width - lead - trail
            }
            Surface::Dock(side) => width(side),
        };
        size(width, viewport.height - theme::TITLE_BAR_HEIGHT)
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
