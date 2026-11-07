use std::ops::Range;
use std::path::Path;

use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Role, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
#[cfg(any(test, feature = "shot"))]
use gpui::{Pixels, Point};
use inkra_vault::Kind;

use crate::file_kind::FileKind;
use crate::one_line::one_line;
use crate::open::Open;
use crate::theme;
use crate::ui::{Color, Icon, IconName, Indicator, ListItem, ListItemStyle};

use super::rows::Item;
use super::{EditState, Row, Sidebar};

impl Sidebar {
    /// Split out only so a test can read the dot.
    pub(super) fn row_indicator(&self, row: &Row, cx: &App) -> Option<Indicator> {
        let notes = self.notes.read(cx);
        Indicator::of(notes.is_dirty(row.id, cx), notes.save_failed(row.id))
    }

    /// The field's row: a `ListItem` like any other, or a pixel taller misplaces every row below it.
    fn render_field(item: usize, edit: &EditState) -> AnyElement {
        ListItem::new(("tree-row", item))
            .role(Role::TreeItem)
            .style(ListItemStyle::Tree)
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
        let indicator = self.row_indicator(&row, cx);
        let label = one_line(&row.name).into_owned();
        // One test for the chevron and `aria_expanded`, so the two cannot disagree.
        let expanded = (row.kind == Kind::Dir).then(|| self.expanded.contains(&row.id));
        let start = match (row.kind, expanded) {
            (_, Some(true)) => Icon::new(IconName::ChevronDown).into_any_element(),
            (_, Some(false)) => Icon::new(IconName::ChevronRight).into_any_element(),
            (Kind::Asset, None) => {
                let icon = match FileKind::of(Path::new(&row.name)) {
                    Some(FileKind::Image) => IconName::Image,
                    Some(FileKind::Note) | None => IconName::File,
                };
                Icon::new(icon).into_any_element()
            }
            (Kind::Dir | Kind::Note, None) => spacer().into_any_element(),
        };
        let selected = self.selected == Some(row.id);
        ListItem::new(("tree-row", item))
            .role(Role::TreeItem)
            .aria_selected(selected)
            .style(ListItemStyle::Tree)
            .when_some(expanded, StatefulInteractiveElement::aria_expanded)
            .indent(row.depth)
            .start(start)
            .selected(selected)
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
                    .text_color(
                        if selected {
                            Color::Selected
                        } else {
                            Color::Default
                        }
                        .rgba(theme::current(cx)),
                    )
                    .child(label),
            )
            .when_some(indicator, |list_item, indicator| {
                list_item.end(
                    div()
                        .debug_selector(move || indicator.selector("tree", item))
                        .child(indicator),
                )
            })
            .into_any_element()
    }
}

/// A note's start slot: a chevron's width of nothing, so every name at one depth starts at one x.
fn spacer() -> Div {
    div().size(theme::ICON_SIZE)
}

#[cfg(any(test, feature = "shot"))]
impl Sidebar {
    /// The stated pitch, not the drawn one: a drift between them misplaces the click, which a test catches.
    pub(crate) fn row_center(&self, ix: f32) -> Option<Point<Pixels>> {
        crate::ui::list_row_center(&self.scroll, theme::TREE_ROW_HEIGHT, ix)
    }
}
