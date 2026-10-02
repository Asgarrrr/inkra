use gpui::{
    App, Div, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div,
};

use super::{UiText, h_flex};
use crate::theme;

/// A panel's first row: its title, ruled like the title row; both panels' headers line up.
#[derive(IntoElement)]
pub(crate) struct PanelHeader {
    base: Div,
    title: SharedString,
}

impl PanelHeader {
    pub(crate) fn new(selector: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            base: h_flex().debug_selector(|| selector.into()),
            title: title.into(),
        }
    }
}

impl RenderOnce for PanelHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.base
            .flex_none()
            // Border-box: the rule sits inside the height.
            .h(theme::PANEL_HEADER_HEIGHT)
            .px(theme::ROW_INSET)
            .ui_text()
            .text_color(theme::placeholder_text())
            .border_b(theme::RULE_WIDTH)
            .border_color(theme::tab_rule())
            .child(div().min_w_0().truncate().child(self.title))
    }
}
