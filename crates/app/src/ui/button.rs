use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};

use super::{ClickHandler, Color, Label, identified, keep_focus};
use crate::theme;

/// A text button with an optional key hint. It owns the look, the role and the click.
#[derive(IntoElement)]
pub(crate) struct Button {
    base: Stateful<Div>,
    label: SharedString,
    hint: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            base: identified(div(), id),
            label: label.into(),
            hint: None,
            on_click: None,
        }
    }

    pub(crate) fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        self.base
            .flex()
            .flex_row()
            .items_center()
            .h(theme::ICON_BUTTON_HEIGHT)
            .px_3()
            .gap_2()
            .rounded(theme::BUTTON_RADIUS)
            .border_1()
            .border_color(colors.border_variant)
            .hover(|style| style.bg(colors.element_hover))
            .role(Role::Button)
            .aria_label(self.label.clone())
            .on_mouse_down(MouseButton::Left, keep_focus)
            .when_some(self.on_click, |button, handler| button.on_click(handler))
            .child(Label::new(self.label))
            .children(self.hint.map(|hint| Label::new(hint).color(Color::Muted)))
    }
}
