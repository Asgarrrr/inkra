//! The centre and each dock, every one a tree of groups (splits spec, S8).

use crate::tree::{Member, Rect, rects};
use crate::{GroupId, Layout, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    Center,
    Dock(Side),
}

impl Layout {
    /// # Panics
    /// If `group` is not in this layout.
    #[must_use]
    pub fn surface_of(&self, group: GroupId) -> Surface {
        [
            Surface::Center,
            Surface::Dock(Side::Left),
            Surface::Dock(Side::Right),
        ]
        .into_iter()
        .find(|&surface| {
            self.member(surface)
                .is_some_and(|member| member.holds(group))
        })
        .unwrap_or_else(|| panic!("{group:?} is not in the layout"))
    }

    #[must_use]
    pub fn dock(&self, side: Side) -> Option<&Member> {
        match side {
            Side::Left => self.left.as_ref(),
            Side::Right => self.right.as_ref(),
        }
    }

    /// Depth-first; empty without a dock.
    #[must_use]
    pub fn dock_groups(&self, side: Side) -> Vec<GroupId> {
        self.dock(side).map_or_else(Vec::new, Member::groups)
    }

    /// `surface` laid out in the unit square, depth-first; empty for an absent dock.
    #[must_use]
    pub fn rects(&self, surface: Surface) -> Vec<(GroupId, Rect)> {
        self.member(surface).map_or_else(Vec::new, rects)
    }

    /// The groups along `surface`'s top, left to right: depth-first order already is.
    #[must_use]
    pub fn top_groups(&self, surface: Surface) -> Vec<GroupId> {
        let mut groups = Vec::new();
        if let Some(member) = self.member(surface) {
            member.collect_top(&mut groups);
        }
        groups
    }

    /// # Panics
    /// If `group` is not in this layout.
    pub(crate) fn holder_mut(&mut self, group: GroupId) -> &mut Member {
        [&mut self.center]
            .into_iter()
            .chain(self.left.as_mut())
            .chain(self.right.as_mut())
            .find(|member| member.holds(group))
            .unwrap_or_else(|| panic!("{group:?} is not in the layout"))
    }

    pub(crate) fn member(&self, surface: Surface) -> Option<&Member> {
        match surface {
            Surface::Center => Some(&self.center),
            Surface::Dock(side) => self.dock(side),
        }
    }

    pub(crate) fn dock_mut(&mut self, side: Side) -> &mut Option<Member> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}
