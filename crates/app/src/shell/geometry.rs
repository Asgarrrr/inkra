//! Where the groups sit in pixels: the docks' ends, each surface's room, a group's rect and its least room (splits spec, P2).

use std::collections::HashMap;

use gpui::{Bounds, Pixels, Size, Window, point, px, size};
use inkra_layout::{Axis, GroupId, Side, Surface};

use super::dock_chrome::width;
use super::nav::nav_width;
use super::{AppShell, LEFT_DOCK_WIDTH, RIGHT_DOCK_WIDTH};
use crate::theme;

const HORIZONTAL_MIN: f32 = 80.;
const VERTICAL_MIN: f32 = 100.;

impl AppShell {
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
    pub(super) fn min_sizes(&self, window: &Window) -> HashMap<(GroupId, Axis), f32> {
        let lights = if window.is_fullscreen() {
            theme::ROW_INSET
        } else {
            theme::TRAFFIC_LIGHT_PADDING
        };
        let leading = lights + theme::ICON_BUTTON_WIDTH;
        let trailing = theme::ICON_BUTTON_WIDTH + theme::ROW_INSET;
        let mut chrome: HashMap<GroupId, Pixels> = HashMap::new();
        let center = theme::TAB_CURVE * 2.
            + nav_width()
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

    /// The px length of `group`'s surface along `axis`: the room `Layout::split` takes.
    pub(super) fn room_along(&self, group: GroupId, axis: Axis, window: &Window) -> f32 {
        let room = self.surface_size(self.layout.surface_of(group), window);
        f32::from(match axis {
            Axis::Horizontal => room.width,
            Axis::Vertical => room.height,
        })
    }

    /// `group`'s room in window coordinates: its share of its own surface, which sits where the
    /// surface does.
    pub(super) fn bounds_of(&self, group: GroupId, window: &Window) -> Option<Bounds<Pixels>> {
        let rect = self.layout.rect_of(group)?;
        let surface = self.layout.surface_of(group);
        let room = self.surface_size(surface, window);
        let left = match surface {
            Surface::Center => self.dock_ends().0,
            Surface::Dock(Side::Left) => px(0.),
            Surface::Dock(Side::Right) => window.viewport_size().width - room.width,
        };
        Some(Bounds::new(
            point(
                left + room.width * rect.x,
                theme::TITLE_BAR_HEIGHT + room.height * rect.y,
            ),
            size(room.width * rect.width, room.height * rect.height),
        ))
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
