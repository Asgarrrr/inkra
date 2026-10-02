//! The chrome's components: the look lives here, the views keep their handlers.

mod assets;
mod icon;
mod icon_button;
mod indicator;
mod list_item;
mod panel_header;
mod tab;
#[cfg(test)]
mod tests;

use gpui::{Div, ElementId, InteractiveElement, Rgba, Stateful, Styled, div};

pub(crate) use assets::{Assets, IconName};
pub(crate) use icon::Icon;
pub(crate) use icon_button::IconButton;
pub(crate) use indicator::Indicator;
pub(crate) use list_item::ListItem;
pub(crate) use panel_header::PanelHeader;
pub(crate) use tab::Tab;

use crate::theme;

pub(crate) fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

pub(crate) fn v_flex() -> Div {
    div().flex().flex_col()
}

/// A component's root: its id is also its debug selector, so `("tab", 3)` reads `tab-3` in tests.
pub(crate) fn identified(base: Div, id: impl Into<ElementId>) -> Stateful<Div> {
    let id = id.into();
    base.debug_selector(|| id.to_string()).id(id)
}

/// One background or a hover, never both: a hover refinement repaints the base (`div.rs:3451`).
pub(crate) fn selectable(base: Stateful<Div>, selected: Option<Rgba>) -> Stateful<Div> {
    match selected {
        Some(background) => base.bg(background),
        None => base.hover(|style| style.bg(theme::row_hover_background())),
    }
}

pub(crate) trait UiText: Styled + Sized {
    fn ui_text(self) -> Self {
        self.text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
    }
}

impl<E: Styled> UiText for E {}
