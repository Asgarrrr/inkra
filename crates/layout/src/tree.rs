//! The centre's tree of groups, normalised after every mutation (splits spec, Layout and L7).

use crate::{GroupId, Layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, left to right.
    Horizontal,
    /// Stacked, top to bottom.
    Vertical,
}

/// Along its axis, a split's child takes `flex / sum` of the split's room.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Group(GroupId),
    Split {
        axis: Axis,
        children: Vec<Member>,
        flexes: Vec<f32>,
    },
}

/// A centre tree as a session stores it: the shape without its ids.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Group,
    Split {
        axis: Axis,
        children: Vec<Shape>,
        flexes: Vec<f32>,
    },
}

impl Shape {
    /// Ids depth-first from `next`.
    fn member(&self, next: &mut u32) -> Member {
        match self {
            Self::Group => {
                let group = GroupId(*next);
                *next += 1;
                Member::Group(group)
            }
            Self::Split {
                axis,
                children,
                flexes,
            } => Member::Split {
                axis: *axis,
                children: children.iter().map(|child| child.member(next)).collect(),
                flexes: flexes.clone(),
            },
        }
    }
}

/// A group's room in the unit square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Zed's flex epsilon (S1).
const EPSILON: f32 = 0.001;

impl Member {
    /// Depth-first.
    pub(crate) fn groups(&self) -> Vec<GroupId> {
        let mut groups = Vec::new();
        self.collect(&mut groups);
        groups
    }

    fn collect(&self, groups: &mut Vec<GroupId>) {
        match self {
            Self::Group(group) => groups.push(*group),
            Self::Split { children, .. } => {
                for child in children {
                    child.collect(groups);
                }
            }
        }
    }

    /// Structural, so no flex can make a lower group top: a vertical split's first child alone
    /// keeps the top edge.
    pub(crate) fn collect_top(&self, groups: &mut Vec<GroupId>) {
        match self {
            Self::Group(group) => groups.push(*group),
            Self::Split {
                axis: Axis::Horizontal,
                children,
                ..
            } => {
                for child in children {
                    child.collect_top(groups);
                }
            }
            Self::Split { children, .. } => {
                if let Some(first) = children.first() {
                    first.collect_top(groups);
                }
            }
        }
    }

    pub(crate) fn holds(&self, group: GroupId) -> bool {
        match self {
            Self::Group(held) => *held == group,
            Self::Split { children, .. } => children.iter().any(|child| child.holds(group)),
        }
    }

    /// Whether `group` was found; it becomes a split on `axis` with `fresh`, which normalising
    /// splices into a parent on `axis`, the two halving the group's flex.
    fn split(&mut self, group: GroupId, fresh: GroupId, axis: Axis) -> bool {
        match self {
            Self::Group(held) if *held == group => {
                *self = Self::Split {
                    axis,
                    children: vec![Self::Group(group), Self::Group(fresh)],
                    flexes: vec![1.0, 1.0],
                };
                true
            }
            Self::Group(_) => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.split(group, fresh, axis)),
        }
    }

    /// The heir: the previous sibling's last group, else the next one's first, which takes the room.
    /// `None` when `group` is this member itself.
    pub(crate) fn remove(&mut self, group: GroupId) -> Option<GroupId> {
        let Self::Split {
            children, flexes, ..
        } = self
        else {
            return None;
        };
        let leaf = Self::Group(group);
        let Some(ix) = children.iter().position(|child| *child == leaf) else {
            return children.iter_mut().find_map(|child| child.remove(group));
        };
        children.remove(ix);
        let flex = flexes.remove(ix);
        let heir_ix = ix.saturating_sub(1);
        flexes[heir_ix] += flex;
        rescale(flexes);
        let held = children[heir_ix].groups();
        if ix == 0 { held.first() } else { held.last() }.copied()
    }
}

impl Layout {
    /// Ids depth-first: the centre's groups, then the left dock's, then the right's; `groups`
    /// lists them in that order. The centre group at `active` is `last_center`, else the first.
    ///
    /// # Panics
    /// If `center` holds no group.
    #[must_use]
    pub fn restored(
        center: &Shape,
        docks: [Option<&Shape>; 2],
        shown: [bool; 2],
        active: usize,
    ) -> Self {
        let mut next = 0;
        let center = normalised(center.member(&mut next));
        let groups = center.groups();
        let Some(&last_center) = groups.get(active).or(groups.first()) else {
            panic!("a restored centre holds a group");
        };
        let [left, right] = docks.map(|dock| dock.map(|shape| normalised(shape.member(&mut next))));
        Self {
            center,
            last_center,
            left,
            right,
            shown,
            next,
        }
    }

    #[must_use]
    pub fn center(&self) -> &Member {
        &self.center
    }

    /// Depth-first.
    #[must_use]
    pub fn center_groups(&self) -> Vec<GroupId> {
        self.center.groups()
    }

    /// A fresh group after `group` on `axis`, on `group`'s surface: a sibling sharing `group`'s
    /// room when its parent splits on `axis`, else a new split in `group`'s place.
    ///
    /// # Panics
    /// If `group` is not in this layout.
    pub fn split(&mut self, group: GroupId, axis: Axis) -> GroupId {
        let fresh = GroupId(self.next);
        let member = self.holder_mut(group);
        member.split(group, fresh, axis);
        normalise(member);
        self.next += 1;
        fresh
    }
}

pub(crate) fn normalise(member: &mut Member) {
    let taken = std::mem::replace(member, Member::Group(GroupId(0)));
    *member = normalised(taken);
}

pub(crate) fn rects(member: &Member) -> Vec<(GroupId, Rect)> {
    let mut rects = Vec::new();
    lay_out(
        member,
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut rects,
    );
    rects
}

fn lay_out(member: &Member, rect: Rect, rects: &mut Vec<(GroupId, Rect)>) {
    match member {
        Member::Group(group) => rects.push((*group, rect)),
        Member::Split {
            axis,
            children,
            flexes,
        } => {
            let sum: f32 = flexes.iter().sum();
            let mut start = 0.0;
            for (child, flex) in children.iter().zip(flexes) {
                let share = flex / sum;
                let part = match axis {
                    Axis::Horizontal => Rect {
                        x: rect.x + rect.width * start,
                        width: rect.width * share,
                        ..rect
                    },
                    Axis::Vertical => Rect {
                        y: rect.y + rect.height * start,
                        height: rect.height * share,
                        ..rect
                    },
                };
                lay_out(child, part, rects);
                start += share;
            }
        }
    }
}

/// Bottom-up: one-child splits collapse, same-axis children splice in, flexes are validated, then
/// rescaled to sum to their count. No rect moves (L7).
pub(crate) fn normalised(member: Member) -> Member {
    let Member::Split {
        axis,
        children,
        flexes,
    } = member
    else {
        return member;
    };
    let children: Vec<_> = children.into_iter().map(normalised).collect();
    let flexes = if valid(&flexes, children.len()) {
        flexes
    } else {
        vec![1.0; children.len()]
    };
    let mut kept = Vec::with_capacity(children.len());
    let mut kept_flexes = Vec::with_capacity(children.len());
    for (child, flex) in children.into_iter().zip(flexes) {
        match child {
            Member::Split {
                axis: inner,
                children,
                flexes,
            } if inner == axis => {
                let scale = flex / flexes.iter().sum::<f32>();
                kept.extend(children);
                kept_flexes.extend(flexes.into_iter().map(|flex| flex * scale));
            }
            child => {
                kept.push(child);
                kept_flexes.push(flex);
            }
        }
    }
    let children = match <[Member; 1]>::try_from(kept) {
        Ok([only]) => return only,
        Err(children) => children,
    };
    rescale(&mut kept_flexes);
    Member::Split {
        axis,
        children,
        flexes: kept_flexes,
    }
}

/// As Zed's `PaneAxis::load`: one per child, each finite and above 0, summing to their count.
fn valid(flexes: &[f32], len: usize) -> bool {
    flexes.len() == len
        && flexes.iter().all(|&flex| flex.is_finite() && flex > 0.0)
        && (flexes.iter().sum::<f32>() - count(len)).abs() <= EPSILON
}

/// Keeps each flex's share.
fn rescale(flexes: &mut [f32]) {
    let scale = count(flexes.len()) / flexes.iter().sum::<f32>();
    for flex in flexes {
        *flex *= scale;
    }
}

#[allow(clippy::cast_precision_loss)]
fn count(len: usize) -> f32 {
    len as f32
}
