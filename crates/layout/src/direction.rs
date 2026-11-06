//! Which side a split opens on, and the group past an edge (splits spec, P3).

use crate::tree::Rect;
use crate::{Axis, GroupId, Layout, Side, Surface};

/// Past an edge, so the point falls in the rect beyond it.
const STEP: f32 = 1e-4;

/// The edge band's depth, as a share of a group's shorter side: Zed's `drop_target_size`.
pub(crate) const DROP_FRACTION: f32 = 0.2;

/// Which side of a group a split opens on, or a neighbour lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Up,
    Down,
    Left,
    Right,
}

impl SplitDirection {
    #[must_use]
    pub fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Up | Self::Down => Axis::Vertical,
        }
    }

    /// Whether a split's fresh group goes first.
    pub(crate) fn before(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
}

/// The edge a drop at `at` in a group of `size` splits towards: the nearest edge, if `at` lies
/// within its band. A tie keeps the first of up, right, down, left, as Zed's `min_by_key`.
#[must_use]
pub fn drop_zone((width, height): (f32, f32), (x, y): (f32, f32)) -> Option<SplitDirection> {
    if width <= 0. || height <= 0. {
        return None;
    }
    let band = width.min(height) * DROP_FRACTION;
    [
        (SplitDirection::Up, y),
        (SplitDirection::Right, width - x),
        (SplitDirection::Down, height - y),
        (SplitDirection::Left, x),
    ]
    .into_iter()
    .min_by(|(_, a), (_, b)| a.total_cmp(b))
    .filter(|&(_, distance)| distance < band)
    .map(|(direction, _)| direction)
}

impl Layout {
    /// The group of `from`'s surface past its edge in `direction`, at `probe` of the way along that
    /// edge. From the centre, Left and Right go on into a drawn dock, which shares the unit height;
    /// from a dock towards the centre, `last_center`.
    ///
    /// # Panics
    /// If `from` is not in this layout.
    #[must_use]
    pub fn neighbour(
        &self,
        from: GroupId,
        direction: SplitDirection,
        probe: f32,
    ) -> Option<GroupId> {
        let surface = self.surface_of(from);
        let rects = self.rects(surface);
        let &(_, rect) = rects.iter().find(|&&(held, _)| held == from)?;
        let (x, y) = past(rect, direction, probe);
        if let Some(group) = holder(&rects, x, y) {
            return Some(group);
        }
        match (surface, direction) {
            (Surface::Center, SplitDirection::Left) => self.dock_holder(Side::Left, 1.0 - STEP, y),
            (Surface::Center, SplitDirection::Right) => self.dock_holder(Side::Right, STEP, y),
            (Surface::Dock(Side::Left), SplitDirection::Right)
            | (Surface::Dock(Side::Right), SplitDirection::Left) => Some(self.last_center),
            _ => None,
        }
    }

    fn dock_holder(&self, side: Side, x: f32, y: f32) -> Option<GroupId> {
        if !self.is_drawn(side) {
            return None;
        }
        holder(&self.rects(Surface::Dock(side)), x, y)
    }
}

/// The point `STEP` past `rect`'s edge in `direction`, at `probe` along it.
fn past(rect: Rect, direction: SplitDirection, probe: f32) -> (f32, f32) {
    match direction {
        SplitDirection::Left => (rect.x - STEP, along(rect.y, rect.height, probe)),
        SplitDirection::Right => (
            rect.x + rect.width + STEP,
            along(rect.y, rect.height, probe),
        ),
        SplitDirection::Up => (along(rect.x, rect.width, probe), rect.y - STEP),
        SplitDirection::Down => (
            along(rect.x, rect.width, probe),
            rect.y + rect.height + STEP,
        ),
    }
}

/// Clamped inside the span, half-open as a rect is: its end is the next rect's start.
fn along(start: f32, length: f32, probe: f32) -> f32 {
    (start + length * probe).clamp(start, start + (length - STEP).max(0.0))
}

/// Half-open, so a point on a shared edge has one holder.
fn holder(rects: &[(GroupId, Rect)], x: f32, y: f32) -> Option<GroupId> {
    rects
        .iter()
        .find(|(_, rect)| {
            rect.x <= x && x < rect.x + rect.width && rect.y <= y && y < rect.y + rect.height
        })
        .map(|&(group, _)| group)
}
