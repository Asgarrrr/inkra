use gpui::{App, IntoElement, RenderOnce, Styled, Window, div};

use crate::theme;

/// A note's unwritten state: edited, or its write failed.
#[derive(IntoElement, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Indicator {
    Dirty,
    Failed,
}

impl Indicator {
    /// `None` for a clean note, so an undo back to the disk text shows nothing.
    pub(crate) fn of(dirty: bool, failed: bool) -> Option<Self> {
        match (dirty, failed) {
            (false, _) => None,
            (true, true) => Some(Self::Failed),
            (true, false) => Some(Self::Dirty),
        }
    }

    pub(crate) fn selector(self, area: &str, ix: usize) -> String {
        match self {
            Self::Dirty => format!("{area}-dot-{ix}"),
            Self::Failed => format!("{area}-failed-{ix}"),
        }
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let color = match self {
            Self::Dirty => theme::dirty_dot(),
            Self::Failed => theme::failed_dot(),
        };
        div().size(theme::DOT_SIZE).rounded_full().bg(color)
    }
}
