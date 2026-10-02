use gpui::{App, Hsla, IntoElement, RenderOnce, Styled, Window, svg};

use super::IconName;
use crate::theme;

#[derive(IntoElement)]
pub(crate) struct Icon {
    name: IconName,
    color: Hsla,
}

impl Icon {
    pub(crate) fn new(name: IconName) -> Self {
        Self {
            name,
            color: theme::icon_muted().into(),
        }
    }

    pub(crate) fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = color.into();
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        // On the svg itself: it paints its alpha mask in its own `text_color` (`svg.rs:150`).
        svg()
            .path(self.name.path())
            .size(theme::ICON_SIZE)
            .flex_none()
            .text_color(self.color)
    }
}
