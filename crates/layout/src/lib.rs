//! Which dock holds each view, which view is active, which dock shows (docks spec).

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dock {
    views: Vec<View>,
    active: usize,
    shown: bool,
}

impl Dock {
    fn holding(view: View) -> Self {
        Self {
            views: vec![view],
            active: 0,
            shown: true,
        }
    }

    #[must_use]
    pub fn views(&self) -> &[View] {
        &self.views
    }

    #[must_use]
    pub fn active(&self) -> Option<View> {
        self.views.get(self.active).copied()
    }

    #[must_use]
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// Rule 3: an empty dock draws nothing, and keeps its flag for when a view comes back.
    #[must_use]
    pub fn is_drawn(&self) -> bool {
        self.shown && !self.views.is_empty()
    }
}

/// A dock as stored: any view names, any index; `Docks::restore` makes it valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDock {
    pub views: Vec<String>,
    pub active: usize,
    pub shown: bool,
}

/// Invariant: each `View` is in exactly one dock, once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Docks {
    left: Dock,
    right: Dock,
}

/// Rule 6: `Files` left, `Backlinks` right, both shown.
impl Default for Docks {
    fn default() -> Self {
        Self {
            left: Dock::holding(View::Files),
            right: Dock::holding(View::Backlinks),
        }
    }
}

impl Docks {
    #[must_use]
    pub fn dock(&self, side: Side) -> &Dock {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    #[must_use]
    pub fn state(&self, side: Side) -> StoredDock {
        let dock = self.dock(side);
        StoredDock {
            views: dock
                .views
                .iter()
                .map(|view| view.name().to_owned())
                .collect(),
            active: dock.active,
            shown: dock.shown,
        }
    }

    /// Keeps each known view's first place, left before right; a missing view goes to its default side.
    #[must_use]
    pub fn restore(left: StoredDock, right: StoredDock) -> Self {
        let mut held = Vec::with_capacity(View::ALL.len());
        let [left, right] = [left, right].map(|stored| {
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
            Dock {
                views,
                active,
                shown: stored.shown,
            }
        });
        let mut docks = Self { left, right };
        let default = Self::default();
        for view in View::ALL {
            if !held.contains(&view) {
                docks.dock_mut(default.side_of(view)).views.push(view);
            }
        }
        docks
    }

    pub fn toggle(&mut self, side: Side) {
        let dock = self.dock_mut(side);
        dock.shown = !dock.shown;
    }

    /// Activates `view` in its dock and shows that dock.
    ///
    /// # Panics
    /// Only on a broken invariant: each view is in exactly one dock.
    pub fn reveal(&mut self, view: View) {
        let dock = self.dock_mut(self.side_of(view));
        let Some(index) = dock.views.iter().position(|&held| held == view) else {
            panic!("each view is in exactly one dock");
        };
        dock.active = index;
        dock.shown = true;
    }

    /// Rules 1 and 2. `index` counts in `side`'s dock after `view` left it.
    ///
    /// # Panics
    /// If `index` exceeds that dock's length after the removal.
    pub fn move_view(&mut self, view: View, side: Side, index: usize) {
        let source = self.dock_mut(self.side_of(view));
        let Some(at) = source.views.iter().position(|&held| held == view) else {
            panic!("each view is in exactly one dock");
        };
        source.views.remove(at);
        if at < source.active {
            source.active -= 1;
        }
        source.active = source.active.min(source.views.len().saturating_sub(1));
        let target = self.dock_mut(side);
        assert!(
            index <= target.views.len(),
            "index {index} is past the end of a dock of {}",
            target.views.len()
        );
        target.views.insert(index, view);
        target.active = index;
        target.shown = true;
    }

    /// Moves `view` to the end of `side`'s dock, as `move_view` does.
    pub fn append(&mut self, view: View, side: Side) {
        let index = self.dock(side).views.len() - usize::from(self.side_of(view) == side);
        self.move_view(view, side, index);
    }

    /// Its dock is drawn and it is that dock's active view.
    #[must_use]
    pub fn is_visible(&self, view: View) -> bool {
        let dock = self.dock(self.side_of(view));
        dock.is_drawn() && dock.active() == Some(view)
    }

    #[must_use]
    pub fn side_of(&self, view: View) -> Side {
        if self.left.views.contains(&view) {
            Side::Left
        } else {
            Side::Right
        }
    }

    fn dock_mut(&mut self, side: Side) -> &mut Dock {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

#[cfg(test)]
mod tests;
