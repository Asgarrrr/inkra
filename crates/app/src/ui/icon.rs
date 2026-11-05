use gpui::{App, IntoElement, RenderOnce, Styled, Window, svg};

use super::{Color, IconName};
use crate::theme;

#[derive(IntoElement)]
pub(crate) struct Icon {
    name: IconName,
    color: Color,
}

impl Icon {
    pub(crate) fn new(name: IconName) -> Self {
        Self {
            name,
            color: Color::Muted,
        }
    }

    pub(crate) fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        // On the svg itself: it paints its alpha mask in its own `text_color` (`svg.rs:150`).
        svg()
            .path(self.name.path())
            .size(theme::ICON_SIZE)
            .flex_none()
            .text_color(self.color.rgba(theme::current(cx)))
    }
}
