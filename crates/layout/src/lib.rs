//! Where each tab group sits and which dock shows (splits spec).

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

/// The centre always holds a group; a dock holds one or none, and keeps its flag either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    center: GroupId,
    left: Option<GroupId>,
    right: Option<GroupId>,
    shown: [bool; 2],
    next: u32,
}

/// Docks rule 6: a group in each dock, both shown.
impl Default for Layout {
    fn default() -> Self {
        Self {
            center: GroupId(0),
            left: Some(GroupId(1)),
            right: Some(GroupId(2)),
            shown: [true, true],
            next: 3,
        }
    }
}

impl Layout {
    #[must_use]
    pub fn center(&self) -> GroupId {
        self.center
    }

    #[must_use]
    pub fn dock(&self, side: Side) -> Option<GroupId> {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
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

    /// `None` for the centre.
    ///
    /// # Panics
    /// If `group` is not in this layout.
    #[must_use]
    pub fn side_of(&self, group: GroupId) -> Option<Side> {
        if group == self.center {
            None
        } else if self.left == Some(group) {
            Some(Side::Left)
        } else if self.right == Some(group) {
            Some(Side::Right)
        } else {
            panic!("{group:?} is not in the layout")
        }
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
        *self.dock_mut(side) = Some(group);
        group
    }

    /// # Panics
    /// On the centre, or a group not in this layout.
    pub fn remove(&mut self, group: GroupId) {
        let Some(side) = self.side_of(group) else {
            panic!("the centre always has a group");
        };
        *self.dock_mut(side) = None;
    }

    /// The centre, then the left dock's, then the right dock's.
    pub fn groups(&self) -> impl Iterator<Item = GroupId> + use<> {
        std::iter::once(self.center)
            .chain(self.left)
            .chain(self.right)
    }

    fn dock_mut(&mut self, side: Side) -> &mut Option<GroupId> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

/// A dock as stored: any view names, any index; `restore_views` makes it valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDock {
    pub views: Vec<String>,
    pub active: usize,
    pub shown: bool,
}

/// Each dock's views and active index, left then right; each `View` lands in exactly one.
///
/// Keeps each known view's first place, left before right; a missing view goes to its default side.
#[must_use]
pub fn restore_views(left: &StoredDock, right: &StoredDock) -> [(Vec<View>, usize); 2] {
    let mut held = Vec::with_capacity(View::ALL.len());
    let mut docks = [left, right].map(|stored| {
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
    });
    for view in View::ALL {
        if !held.contains(&view) {
            docks[view.default_side().index()].0.push(view);
        }
    }
    docks
}

#[cfg(test)]
mod tests;
