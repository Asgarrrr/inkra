use gpui::{
    AnyView, App, AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div,
};

use super::{Label, LabelSize, UiText};
use crate::theme;

/// A hover's name for a control with no text.
pub(crate) struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub(crate) fn text(
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |_, cx| cx.new(|_| Tooltip { text: text.clone() }).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let text = self.text.clone();
        // Drawn at the window's root, so it inherits no text style; the inset keeps it off the pointer, as Zed's.
        div()
            .debug_selector(move || format!("tooltip-{text}"))
            .ui_text()
            .pl_2()
            .pt_2p5()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .bg(colors.elevated_background)
                    .border_1()
                    .border_color(colors.border)
                    .rounded_md()
                    .shadow_md()
                    .child(Label::new(self.text.clone()).size(LabelSize::Small)),
            )
    }
}
