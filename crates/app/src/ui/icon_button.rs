use gpui::{
    App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder,
};

use super::{Icon, IconName, identified};
use crate::theme;

/// A button that is only an icon. It owns the look and the role; a caller attaches the handlers.
#[derive(IntoElement)]
pub(crate) struct IconButton {
    base: Stateful<Div>,
    icon: IconName,
    /// The accessible name: an icon has no text.
    label: SharedString,
    /// `None` for a plain button; `Some(on)` for a toggle, whose icon is at full strength when on.
    toggled: Option<bool>,
    radius: Pixels,
}

impl IconButton {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            base: identified(div(), id),
            icon,
            label: label.into(),
            toggled: None,
            radius: theme::BUTTON_RADIUS,
        }
    }

    pub(crate) fn toggled(mut self, on: bool) -> Self {
        self.toggled = Some(on);
        self
    }

    pub(crate) fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }
}

impl InteractiveElement for IconButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for IconButton {}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let color = if self.toggled == Some(true) {
            theme::icon_active()
        } else {
            theme::icon_muted()
        };
        self.base
            .flex_none()
            .w(theme::ICON_BUTTON_WIDTH)
            .h(theme::ICON_BUTTON_HEIGHT)
            .flex()
            .items_center()
            .justify_center()
            .rounded(self.radius)
            .hover(|style| style.bg(theme::button_hover_background()))
            .role(Role::Button)
            .aria_label(self.label)
            .when_some(self.toggled, |button, on| button.aria_toggled(on.into()))
            // Focus stays where the user put it: gpui's root focus-on-press (`div.rs:2765-2780`) is prevented.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .child(Icon::new(self.icon).color(color))
    }
}
