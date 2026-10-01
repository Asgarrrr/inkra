//! The chrome's components: the look lives here, the views keep their handlers.

mod assets;
mod icon;
mod list_item;
#[cfg(test)]
mod tests;

use gpui::{Div, Styled, div};

pub(crate) use assets::{Assets, IconName};
pub(crate) use icon::Icon;
pub(crate) use list_item::ListItem;

use crate::theme;

pub(crate) fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

pub(crate) fn v_flex() -> Div {
    div().flex().flex_col()
}

pub(crate) trait UiText: Styled + Sized {
    fn ui_text(self) -> Self {
        self.text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
    }
}

impl<E: Styled> UiText for E {}
