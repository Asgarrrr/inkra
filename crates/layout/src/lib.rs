//! Which dock holds each view, which view is active, which dock shows (docks spec).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Files,
    Backlinks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
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

    /// Its dock is shown and it is that dock's active view.
    #[must_use]
    pub fn is_visible(&self, view: View) -> bool {
        let dock = self.dock(self.side_of(view));
        dock.shown && dock.active() == Some(view)
    }

    fn dock_mut(&mut self, side: Side) -> &mut Dock {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn side_of(&self, view: View) -> Side {
        if self.left.views.contains(&view) {
            Side::Left
        } else {
            Side::Right
        }
    }
}

#[cfg(test)]
mod tests;
