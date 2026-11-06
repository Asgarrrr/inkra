//! Where each tab group sits and which dock shows (splits spec).

mod direction;
mod resize;
mod surface;
mod tree;

pub use direction::SplitDirection;
pub use resize::SplitPath;
pub use surface::Surface;
pub use tree::{Axis, Member, Rect, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Files,
    Backlinks,
}

impl View {
    pub const ALL: [View; 2] = [View::Files, View::Backlinks];

    /// The name the session file stores.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Backlinks => "backlinks",
        }
    }

    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.name() == name)
    }

    /// Docks rule 6: where the view goes when no dock holds it.
    #[must_use]
    pub fn default_side(self) -> Side {
        match self {
            Self::Files => Side::Left,
            Self::Backlinks => Side::Right,
        }
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

/// A dock as stored: any view names, any index; `restore_views` makes it valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredDock {
    pub views: Vec<String>,
    pub active: usize,
}

/// Each dock group's views and active index, left groups then right, each side's depth-first;
/// each `View` lands in exactly one, and a group emptied of views stays for the caller to remove.
///
/// Keeps each known view's first place; a missing view goes to its default side's first group,
/// or a fresh one when that side has none.
#[must_use]
pub fn restore_views(docks: [&[StoredDock]; 2]) -> [Vec<(Vec<View>, usize)>; 2] {
    let mut held = Vec::with_capacity(View::ALL.len());
    let mut docks = docks.map(|groups| {
        groups
            .iter()
            .map(|stored| restore_group(stored, &mut held))
            .collect::<Vec<_>>()
    });
    for view in View::ALL {
        if !held.contains(&view) {
            let side = &mut docks[view.default_side().index()];
            match side.first_mut() {
                Some((views, _)) => views.push(view),
                None => side.push((vec![view], 0)),
            }
        }
    }
    docks
}

/// `stored`'s known views not in `held`, which takes them.
fn restore_group(stored: &StoredDock, held: &mut Vec<View>) -> (Vec<View>, usize) {
    let (views, stored_ixs): (Vec<_>, Vec<_>) = stored
        .views
        .iter()
        .enumerate()
        .filter_map(|(stored_ix, name)| {
            let view = View::named(name).filter(|view| !held.contains(view))?;
            held.push(view);
            Some((view, stored_ix))
        })
        .unzip();
    // The stored active view, else its nearest kept entry to the left, else the first.
    let active = stored_ixs
        .iter()
        .rposition(|&stored_ix| stored_ix <= stored.active)
        .unwrap_or(0);
    (views, active)
}

#[cfg(test)]
mod tests;
