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
    fn remove(&mut self, group: GroupId) -> Option<GroupId> {
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
    #[must_use]
    pub fn center(&self) -> &Member {
        &self.center
    }

    /// Depth-first.
    #[must_use]
    pub fn center_groups(&self) -> Vec<GroupId> {
        self.center.groups()
    }

    /// A fresh group after `group` on `axis`: a sibling sharing `group`'s room when its parent
    /// splits on `axis`, else a new split in `group`'s place.
    ///
    /// # Panics
    /// If `group` is not in the centre.
    pub fn split(&mut self, group: GroupId, axis: Axis) -> GroupId {
        let fresh = GroupId(self.next);
        let found = self.center.split(group, fresh, axis);
        assert!(found, "{group:?} is not in the centre");
        self.next += 1;
        self.normalise();
        fresh
    }

    /// # Panics
    /// If `group` is the centre's only group.
    pub(crate) fn remove_center(&mut self, group: GroupId) -> GroupId {
        let Some(heir) = self.center.remove(group) else {
            panic!("the centre always has a group");
        };
        self.normalise();
        heir
    }

    /// The centre laid out in the unit square, depth-first.
    #[must_use]
    pub fn rects(&self) -> Vec<(GroupId, Rect)> {
        rects(&self.center)
    }

    /// The centre groups along the window's top, left to right: depth-first order already is.
    #[must_use]
    pub fn top_groups(&self) -> Vec<GroupId> {
        self.rects()
            .into_iter()
            .filter(|(_, rect)| rect.y < EPSILON)
            .map(|(group, _)| group)
            .collect()
    }

    fn normalise(&mut self) {
        let center = std::mem::replace(&mut self.center, Member::Group(self.last_center));
        self.center = normalised(center);
    }
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
