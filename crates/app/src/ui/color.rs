use gpui::Rgba;

use crate::theme::Theme;

/// A text or icon colour by role; the theme decides the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Color {
    #[default]
    Default,
    Selected,
    Muted,
    Placeholder,
    Highlight,
    Accent,
    Error,
    Warning,
}

impl Color {
    pub(crate) fn rgba(self, theme: &Theme) -> Rgba {
        let colors = &theme.colors;
        match self {
            Self::Default => colors.text,
            Self::Selected => colors.text_selected,
            Self::Muted => colors.text_muted,
            Self::Placeholder => colors.text_placeholder,
            Self::Highlight => colors.text_highlight,
            Self::Accent => colors.accent,
            Self::Error => colors.error,
            Self::Warning => colors.warning,
        }
    }
}
