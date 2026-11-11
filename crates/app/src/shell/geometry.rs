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

/// One fixed piece of chrome at an end of the title row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EndPiece {
    Lights,
    Toggle(Side),
    Inset,
}

impl EndPiece {
    pub(super) fn width(self, lights: Pixels) -> Pixels {
        match self {
            Self::Lights => lights,
            Self::Toggle(_) => theme::ICON_BUTTON_WIDTH,
            Self::Inset => theme::ROW_INSET,
        }
    }
}

/// The pieces a top group's title segment draws before and after its own content, in order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Ends {
    pub(super) lead: [Option<EndPiece>; 2],
    pub(super) trail: [Option<EndPiece>; 2],
}

/// The traffic lights' room; full screen hides the native buttons.
pub(super) fn lights_width(window: &Window) -> Pixels {
    if window.is_fullscreen() {
        theme::ROW_INSET
    } else {
        theme::TRAFFIC_LIGHT_PADDING
    }
}

impl AppShell {
    /// The main row's ends taken by the drawn docks.
    pub(super) fn dock_ends(&self) -> (Pixels, Pixels) {
        let drawn = |side, width| {
            if self.is_dock_drawn(side) {
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

    /// The end chrome of top group `ix` of `count` on `surface`: each window end goes to its dock when drawn, else to the centre.
    pub(super) fn title_ends(&self, surface: Surface, ix: usize, count: usize) -> Ends {
        let (first, last) = (ix == 0, ix + 1 == count);
        let toggle = |side| self.has_dock_toggle(side).then_some(EndPiece::Toggle(side));
        let window_start = [Some(EndPiece::Lights), toggle(Side::Left)];
        let window_end = [toggle(Side::Right), Some(EndPiece::Inset)];
        let (lead, trail) = match surface {
            Surface::Center => (
                (first && !self.is_dock_drawn(Side::Left)).then_some(window_start),
                (last && !self.is_dock_drawn(Side::Right)).then_some(window_end),
            ),
            Surface::Dock(side) if !self.is_dock_drawn(side) => (None, None),
            Surface::Dock(Side::Left) => (first.then_some(window_start), None),
            Surface::Dock(Side::Right) => (
                first.then_some([Some(EndPiece::Inset), None]),
                last.then_some(window_end),
            ),
        };
        Ends {
            lead: lead.unwrap_or_default(),
            trail: trail.unwrap_or_default(),
        }
    }

    /// Each group's least room along each axis: the floor, or the title-row chrome it draws if wider.
    pub(super) fn min_sizes(&self, window: &Window) -> HashMap<(GroupId, Axis), f32> {
        let lights = lights_width(window);
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
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a count far below 2^24 converts to f32 exactly"
                )]
                let views = self.group(group).views().count() as f32;
                chrome.insert(
                    group,
                    theme::ICON_BUTTON_WIDTH * views + theme::TITLE_DRAG_MIN_WIDTH,
                );
            }
        }
        for surface in [
            Surface::Center,
            Surface::Dock(Side::Left),
            Surface::Dock(Side::Right),
        ] {
            let top = self.layout.top_groups(surface);
            for (ix, group) in top.iter().enumerate() {
                let Ends { lead, trail } = self.title_ends(surface, ix, top.len());
                let ends: Pixels = lead
                    .into_iter()
                    .chain(trail)
                    .flatten()
                    .map(|piece| piece.width(lights))
                    .sum();
                if let Some(width) = chrome.get_mut(group) {
                    *width += ends;
                }
            }
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
