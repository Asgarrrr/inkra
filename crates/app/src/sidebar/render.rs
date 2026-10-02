use std::ops::Range;

#[cfg(any(test, feature = "shot"))]
use gpui::Pixels;
use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window, div, prelude::FluentBuilder,
};
use inkra_vault::Kind;

use crate::one_line::one_line;
use crate::open::Open;
use crate::theme;
use crate::ui::{Icon, IconName, Indicator, ListItem};

use super::rows::Item;
use super::{EditState, Row, Sidebar};

impl Sidebar {
    /// Split out only so a test can read the dot.
    pub(super) fn row_is_dirty(&self, row: &Row, cx: &App) -> bool {
        self.notes.read(cx).is_dirty(row.id, cx)
    }

    /// The field's row: a `ListItem` like any other, or a pixel taller misplaces every row below it.
    fn render_field(item: usize, edit: &EditState) -> AnyElement {
        ListItem::new(("tree-row", item))
            .indent(edit.depth)
            .start(spacer())
            .selected(true)
            .child(edit.field.clone())
            .into_any_element()
    }

    pub(super) fn render_rows(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|item| match self.rows.item(item) {
                Some(Item::Row(row)) => self.render_row(item, &self.rows.visible().row(row), cx),
                Some(Item::Field(edit)) => Self::render_field(item, edit),
                None => div().into_any_element(),
            })
            .collect()
    }

    fn render_row(&self, item: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let row = row.clone();
        // Reading inside the draw is what repaints the dot; `Entity::cached` would break it.
        let dirty = self.row_is_dirty(&row, cx);
        let label = one_line(&row.name).into_owned();
        let start = match row.kind {
            Kind::Dir if self.expanded.contains(&row.id) => {
                Icon::new(IconName::ChevronDown).into_any_element()
            }
            Kind::Dir => Icon::new(IconName::ChevronRight).into_any_element(),
            Kind::Note => spacer().into_any_element(),
        };
        ListItem::new(("tree-row", item))
            .indent(row.depth)
            .start(start)
            .selected(self.selected == Some(row.id))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |sidebar, event: &MouseDownEvent, window, cx| {
                    // Focus first, so the tree's bindings resolve from the next keystroke.
                    window.focus(&sidebar.focus_handle, cx);
                    // The press's modifiers: the row acts at mouse-down, a picker row at the release.
                    sidebar.activate(&row, Open::from_click(event.modifiers), cx);
                }),
            )
            .child(
                div()
                    .debug_selector(|| format!("tree-name-{item}"))
                    .whitespace_nowrap()
                    .text_color(theme::sidebar_text())
                    .child(label),
            )
            .when(dirty, |list_item| {
                list_item.end(
                    div()
                        .debug_selector(move || format!("tree-dot-{item}"))
                        .child(Indicator),
                )
            })
            .into_any_element()
    }
}

/// A note's start slot: a chevron's width of nothing, so every name at one depth starts at one x.
fn spacer() -> Div {
    div().size(theme::ICON_SIZE)
}

/// Not `#[cfg(test)]`-gated: the shot runner needs it in a normal build too.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn row_center(ix: f32) -> Pixels {
    theme::UI_LINE_HEIGHT * (ix + 0.5)
}
