//! A separator's drag and double-click: flexes moved between two neighbours (splits spec, P2).

use crate::tree::{EPSILON, count};
use crate::{Axis, GroupId, Layout, Member, Surface};

/// A member of `surface`'s tree: the child indices from its root, which `steps` empty names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPath {
    pub surface: Surface,
    pub steps: Vec<usize>,
}

impl Layout {
    #[must_use]
    pub fn member_at(&self, path: &SplitPath) -> Option<&Member> {
        path.steps
            .iter()
            .try_fold(self.member(path.surface)?, |member, &ix| match member {
                Member::Split { children, .. } => children.get(ix),
                Member::Group(_) => None,
            })
    }

    fn member_at_mut(&mut self, path: &SplitPath) -> Option<&mut Member> {
        let root = match path.surface {
            Surface::Center => &mut self.center,
            Surface::Dock(side) => self.dock_mut(side).as_mut()?,
        };
        path.steps
            .iter()
            .try_fold(root, |member, &ix| match member {
                Member::Split { children, .. } => children.get_mut(ix),
                Member::Group(_) => None,
            })
    }

    /// Moves the boundary before child `ix` by `delta` px along the split's axis, in a container
    /// `size` px long. Each neighbour stops at its minimum, or where it is if already under it;
    /// the flexes keep their sum. Whether a flex changed.
    pub fn resize(
        &mut self,
        path: &SplitPath,
        ix: usize,
        delta: f32,
        size: f32,
        min: impl Fn(GroupId, Axis) -> f32,
    ) -> bool {
        let Some(Member::Split {
            axis,
            children,
            flexes,
        }) = self.member_at_mut(path)
        else {
            return false;
        };
        if ix == 0 || ix >= children.len() || size <= 0.0 {
            return false;
        }
        let unit = size / count(children.len());
        let (a, b) = (flexes[ix - 1], flexes[ix]);
        let (a_px, b_px) = (a * unit, b * unit);
        let low = minimum(&children[ix - 1], *axis, &min).min(a_px);
        // `max`: rounding can put the bound a hair under `low` when both sides are undersized.
        let high = (a_px + b_px - minimum(&children[ix], *axis, &min).min(b_px)).max(low);
        let resized = (a_px + delta).clamp(low, high) / unit;
        if (resized - a).abs() < EPSILON {
            return false;
        }
        flexes[ix - 1] = resized;
        flexes[ix] = a + b - resized;
        true
    }

    /// Whether a flex changed.
    pub fn equalize(&mut self, path: &SplitPath) -> bool {
        let Some(Member::Split { flexes, .. }) = self.member_at_mut(path) else {
            return false;
        };
        let equal = vec![1.0; flexes.len()];
        if *flexes == equal {
            return false;
        }
        *flexes = equal;
        true
    }
}

/// Along `axis`: a row of `axis` needs each child's room, a split across it its widest child's.
pub(crate) fn minimum(member: &Member, axis: Axis, min: &impl Fn(GroupId, Axis) -> f32) -> f32 {
    match member {
        Member::Group(group) => min(*group, axis),
        Member::Split {
            axis: inner,
            children,
            ..
        } => {
            let each = children.iter().map(|child| minimum(child, axis, min));
            if *inner == axis {
                each.sum()
            } else {
                each.fold(0.0, f32::max)
            }
        }
    }
}
