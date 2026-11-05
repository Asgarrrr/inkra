//! The chrome's components: the look lives here, the views keep their handlers.

mod assets;
mod color;
mod icon;
mod icon_button;
mod indicator;
mod label;
mod list_item;
mod panel_header;
mod tab;
#[cfg(test)]
mod tests;
mod tooltip;

use gpui::{
    App, Div, ElementId, InteractiveElement, MouseDownEvent, Rgba, Stateful, Styled, Window, div,
};

pub(crate) use assets::{Assets, IconName};
pub(crate) use color::Color;
pub(crate) use icon::Icon;
pub(crate) use icon_button::IconButton;
pub(crate) use indicator::Indicator;
pub(crate) use label::{Label, LabelSize};
pub(crate) use list_item::{ListItem, ListItemStyle};
pub(crate) use panel_header::PanelHeader;
pub(crate) use tab::Tab;
pub(crate) use tooltip::Tooltip;

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

/// A left-press listener for chrome that is not a text target: the root would take focus from the editor on the press (`div.rs:2765-2780`).
pub(crate) fn keep_focus(_: &MouseDownEvent, window: &mut Window, _: &mut App) {
    window.prevent_default();
}

/// One background or a hover, never both: a hover refinement repaints the base (`div.rs:3451`).
pub(crate) fn selectable(
    base: Stateful<Div>,
    selected: Option<Rgba>,
    hover: Rgba,
) -> Stateful<Div> {
    match selected {
        Some(background) => base.bg(background),
        None => base.hover(|style| style.bg(hover)),
    }
}

/// Where row `ix` of a uniform list was drawn last, at `pitch` per row; `None` before the list is drawn.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn list_row_center(
    scroll: &gpui::UniformListScrollHandle,
    pitch: gpui::Pixels,
    ix: f32,
) -> Option<gpui::Point<gpui::Pixels>> {
    let list = scroll.0.borrow().base_handle.clone();
    let bounds = list.bounds();
    // Zero-sized until the list's first prepaint.
    if bounds.is_empty() {
        return None;
    }
    let top = bounds.top() + list.offset().y;
    Some(gpui::point(bounds.center().x, top + pitch * (ix + 0.5)))
}

pub(crate) trait UiText: Styled + Sized {
    fn ui_text(self) -> Self {
        self.text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
    }
}

impl<E: Styled> UiText for E {}
