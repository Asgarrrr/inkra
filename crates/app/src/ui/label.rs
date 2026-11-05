use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div};

use super::Color;
use crate::theme;

#[derive(Clone, Copy, Default)]
pub(crate) enum LabelSize {
    #[default]
    Ui,
    Small,
}

/// Plain text in one line box of its own, so a row holding it cannot clip it.
#[derive(IntoElement)]
pub(crate) struct Label {
    text: SharedString,
    color: Color,
    size: LabelSize,
}

impl Label {
    pub(crate) fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            color: Color::Default,
            size: LabelSize::Ui,
        }
    }

    pub(crate) fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub(crate) fn size(mut self, size: LabelSize) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (size, line_height) = match self.size {
            LabelSize::Ui => (theme::UI_TEXT_SIZE, theme::UI_LINE_HEIGHT),
            LabelSize::Small => (
                theme::HEADER_COUNT_TEXT_SIZE,
                theme::HEADER_COUNT_LINE_HEIGHT,
            ),
        };
        div()
            .text_color(self.color.rgba(theme::current(cx)))
            .text_size(size)
            .line_height(line_height)
            .child(self.text)
    }
}
