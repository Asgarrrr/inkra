//! Where each tab group sits and which dock shows (splits spec).

mod direction;
mod flex;
mod resize;
mod surface;
mod tree;

pub use direction::{SplitDirection, drop_zone};
pub use resize::SplitPath;
pub use surface::Surface;
pub use tree::{Axis, Member, Rect, Shape};

/// A dock view, by the name the session file stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId(&'static str);

impl ViewId {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

/// Handed out only by a `Layout`, never twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(u32);

impl GroupId {
    /// Unique within one layout, so an element id can carry it.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// The centre always holds a group; a dock holds a tree or none, and keeps its flag either way.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    center: Member,
    last_center: GroupId,
    left: Option<Member>,
    right: Option<Member>,
    shown: [bool; 2],
    next: u32,
}

/// Docks rule 6: a group in each dock, both shown.
impl Default for Layout {
    fn default() -> Self {
        Self {
            center: Member::Group(GroupId(0)),
            last_center: GroupId(0),
            left: Some(Member::Group(GroupId(1))),
            right: Some(Member::Group(GroupId(2))),
            shown: [true, true],
            next: 3,
        }
    }
}

impl Layout {
    /// The centre group that feeds the note consumers.
    #[must_use]
    pub fn last_center(&self) -> GroupId {
        self.last_center
    }

    /// # Panics
    /// If `group` is not in the centre.
    pub fn activate(&mut self, group: GroupId) {
        assert!(self.center.holds(group), "{group:?} is not in the centre");
        self.last_center = group;
    }

    #[must_use]
    pub fn is_shown(&self, side: Side) -> bool {
        self.shown[side.index()]
    }

    /// Docks rule 3: an empty dock draws nothing.
    #[must_use]
    pub fn is_drawn(&self, side: Side) -> bool {
        self.is_shown(side) && self.dock(side).is_some()
    }

    pub fn toggle(&mut self, side: Side) {
        self.shown[side.index()] ^= true;
    }

    pub fn show(&mut self, side: Side) {
        self.shown[side.index()] = true;
    }

    /// # Panics
    /// If that dock already holds a group.
    pub fn add_dock(&mut self, side: Side) -> GroupId {
        assert!(
            self.dock(side).is_none(),
            "the {side:?} dock already holds a group"
        );
        let group = GroupId(self.next);
        self.next += 1;
        *self.dock_mut(side) = Some(Member::Group(group));
        group
    }

    /// A group's room goes to its previous sibling, else its next, whose nearest group it returns
    /// (L6). That heir becomes `last_center` in a removed `last_center`'s place; a dock's only
    /// group leaves the dock empty and returns `None`.
    ///
    /// # Panics
    /// On the centre's only group, or a group not in this layout.
    pub fn remove(&mut self, group: GroupId) -> Option<GroupId> {
        match self.surface_of(group) {
            Surface::Center => {
                let Some(heir) = self.center.remove(group) else {
                    panic!("the centre always has a group");
                };
                tree::normalise(&mut self.center);
                if self.last_center == group {
                    self.last_center = heir;
                }
                Some(heir)
            }
            Surface::Dock(side) => {
                let dock = self.dock_mut(side);
                let heir = dock.as_mut().and_then(|member| member.remove(group));
                match dock {
                    Some(member) if heir.is_some() => tree::normalise(member),
                    _ => *dock = None,
                }
                heir
            }
        }
    }

    /// L3: every group in this layout but the centre's only one.
    #[must_use]
    pub fn removable(&self, group: GroupId) -> bool {
        self.center != Member::Group(group) && self.groups().any(|held| held == group)
    }

    /// The centre depth-first, then the left dock's, then the right dock's.
    pub fn groups(&self) -> impl Iterator<Item = GroupId> {
        [
            self.center_groups(),
            self.dock_groups(Side::Left),
            self.dock_groups(Side::Right),
        ]
        .into_iter()
        .flatten()
    }
}

/// A dock as stored: one entry per stored tab, `None` for a tab that names no view;
/// any name, any index; `restore_views` makes it valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredDock {
    pub views: Vec<Option<String>>,
    pub active: usize,
}

/// Each dock group's views and active index, left groups then right, each side's depth-first;
/// each catalog view lands in exactly one, and a group emptied of views stays for the caller to
/// remove.
///
/// Keeps each known view's first place; a missing view goes to its catalog side's first group,
/// or a fresh one when that side has none, in catalog order.
#[must_use]
pub fn restore_views(
    docks: [&[StoredDock]; 2],
    catalog: &[(ViewId, Side)],
) -> [Vec<(Vec<ViewId>, usize)>; 2] {
    let mut held = Vec::with_capacity(catalog.len());
    let mut docks = docks.map(|groups| {
        groups
            .iter()
            .map(|stored| restore_group(stored, catalog, &mut held))
            .collect::<Vec<_>>()
    });
    for &(view, side) in catalog {
        if !held.contains(&view) {
            let side = &mut docks[side.index()];
            match side.first_mut() {
                Some((views, _)) => views.push(view),
                None => side.push((vec![view], 0)),
            }
        }
    }
    docks
}

/// The stored active entry, else its nearest kept entry to the left, else the first.
/// `kept` holds the surviving stored indices, ascending.
#[must_use]
pub fn kept_active(kept: &[usize], active: usize) -> usize {
    kept.iter().rposition(|&ix| ix <= active).unwrap_or(0)
}

/// `stored`'s catalog views not in `held`, which takes them.
fn restore_group(
    stored: &StoredDock,
    catalog: &[(ViewId, Side)],
    held: &mut Vec<ViewId>,
) -> (Vec<ViewId>, usize) {
    let (views, stored_ixs): (Vec<_>, Vec<_>) = stored
        .views
        .iter()
        .enumerate()
        .filter_map(|(stored_ix, name)| {
            let name = name.as_deref()?;
            let view = catalog
                .iter()
                .map(|&(view, _)| view)
                .find(|view| view.name() == name)
                .filter(|view| !held.contains(view))?;
            held.push(view);
            Some((view, stored_ix))
        })
        .unzip();
    let active = kept_active(&stored_ixs, stored.active);
    (views, active)
}

#[cfg(test)]
mod tests;
