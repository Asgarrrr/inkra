use gpui::{
    App, Div, FontWeight, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, SharedString, Styled, Window, div, prelude::FluentBuilder,
};

use super::{Color, Label, LabelSize, UiText, h_flex};
use crate::theme;

/// A panel's first row: its title and an optional count, ruled like the title row.
#[derive(IntoElement)]
pub(crate) struct PanelHeader {
    base: Div,
    title: SharedString,
    count: Option<usize>,
}

impl PanelHeader {
    pub(crate) fn new(selector: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            base: h_flex().debug_selector(|| selector.into()),
            title: title.into(),
            count: None,
        }
    }

    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
}

impl InteractiveElement for PanelHeader {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for PanelHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = theme::current(cx);
        let colors = theme.colors;
        self.base
            .flex_none()
            // Border-box: the rule sits inside the height.
            .h(theme::PANEL_HEADER_HEIGHT)
            .px(theme::ROW_INSET)
            .ui_text()
            .border_b(theme::RULE_WIDTH)
            .border_color(colors.border_variant)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(Color::Default.rgba(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title),
            )
            .when_some(self.count, |header, count| {
                header.child(
                    div()
                        .debug_selector(move || format!("panel-header-count-{count}"))
                        .flex_none()
                        .ml_1p5()
                        .px_1p5()
                        .rounded_full()
                        .bg(colors.chrome_element_hover)
                        .child(
                            Label::new(count.to_string())
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                        ),
                )
            })
    }
}
