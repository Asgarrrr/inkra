use gpui::{
    AnyElement, App, Div, FontWeight, InteractiveElement, Interactivity, IntoElement,
    ParentElement, RenderOnce, SharedString, Styled, Window, div, prelude::FluentBuilder,
};

use super::{Color, Label, LabelSize, UiText, h_flex};
use crate::theme;

/// A panel's first row, ruled like the title row: a title and an optional count, or a bar of children.
#[derive(IntoElement)]
pub(crate) struct PanelHeader {
    base: Div,
    title: Option<SharedString>,
    count: Option<usize>,
    children: Vec<AnyElement>,
}

impl PanelHeader {
    pub(crate) fn new(selector: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::bar(selector)
        }
    }

    pub(crate) fn bar(selector: &'static str) -> Self {
        Self {
            base: h_flex().debug_selector(|| selector.into()),
            title: None,
            count: None,
            children: Vec::new(),
        }
    }

    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
}

impl ParentElement for PanelHeader {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
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
            .when_some(self.title, |header, title| {
                header.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(Color::Default.rgba(theme))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
            })
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
            .children(self.children)
    }
}
