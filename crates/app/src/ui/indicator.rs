use gpui::{App, IntoElement, RenderOnce, Styled, Window, div};

use super::Color;
use crate::theme;

/// A dot: a note's unwritten state (edited, or its write failed), or the current entry of a list.
#[derive(IntoElement, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Indicator {
    Dirty,
    Failed,
    Current,
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
            Self::Current => format!("{area}-current-{ix}"),
        }
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = match self {
            Self::Dirty => Color::Muted,
            Self::Failed => Color::Error,
            Self::Current => Color::Accent,
        };
        div()
            .size(theme::DOT_SIZE)
            .rounded_full()
            .bg(color.rgba(theme::current(cx)))
    }
}
