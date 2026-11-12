use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement, Interactivity, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder,
};

use super::{ClickHandler, Color, Icon, IconName, identified, keep_focus};
use crate::theme;

/// A button that is only an icon. It owns the look, the role and the click; a caller attaches the rest.
#[derive(IntoElement)]
pub(crate) struct IconButton {
    base: Stateful<Div>,
    id: ElementId,
    icon: IconName,
    /// The accessible name: an icon has no text.
    label: SharedString,
    /// `None` for a plain button; `Some(on)` for a toggle, whose icon is at full strength when on.
    toggled: Option<bool>,
    radius: Pixels,
    /// `None` for the standard hit area; `Some(side)` for a square, smaller box.
    square: Option<Pixels>,
    /// Dimmed and inert: no hover, and the click is dropped. It is drawn as `{id}-disabled`, so a test reads the state.
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Self {
        let id = id.into();
        Self {
            base: identified(div(), id.clone()),
            id,
            icon,
            label: label.into(),
            toggled: None,
            radius: theme::BUTTON_RADIUS,
            square: None,
            disabled: false,
            on_click: None,
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

    pub(crate) fn square(mut self, side: Pixels) -> Self {
        self.square = Some(side);
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl InteractiveElement for IconButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for IconButton {}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let color = if self.toggled == Some(true) {
            Color::Default
        } else {
            Color::Muted
        };
        let button = self
            .base
            .flex_none()
            .w(self.square.unwrap_or(theme::ICON_BUTTON_WIDTH))
            .h(self.square.unwrap_or(theme::ICON_BUTTON_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .rounded(self.radius)
            .role(Role::Button)
            .aria_label(self.label)
            .when_some(self.toggled, |button, on| button.aria_toggled(on.into()))
            .on_mouse_down(MouseButton::Left, keep_focus)
            .child(Icon::new(self.icon).color(color));
        if self.disabled {
            let id = self.id;
            button
                .debug_selector(move || format!("{id}-disabled"))
                .opacity(theme::DISABLED_OPACITY)
                .cursor_not_allowed()
        } else {
            button
                .hover(|style| style.bg(colors.ghost_element_hover))
                .when_some(self.on_click, |button, handler| button.on_click(handler))
        }
    }
}
