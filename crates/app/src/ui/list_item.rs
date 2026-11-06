use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};

use super::{UiText, h_flex, identified, selectable};
use crate::theme;

/// Where a row sits: the surface decides its height, shape and lighting.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ListItemStyle {
    /// On the chrome: `TREE_ROW_HEIGHT` tall, rounded, lit with the chrome's row colour.
    Tree,
    /// In a list padded by `MENU_INSET`: its own line box, rounded, lit with the element colours.
    #[default]
    Menu,
}

/// A row of a list. It owns background, hover and padding; a caller attaches handlers, role,
/// labels and content, and a caller's `.hover()` trips gpui's "hover style already set" assert.
#[derive(IntoElement)]
pub(crate) struct ListItem {
    base: Stateful<Div>,
    indent: usize,
    start: Option<AnyElement>,
    children: Vec<AnyElement>,
    end: Option<AnyElement>,
    selected: bool,
    style: ListItemStyle,
}

impl ListItem {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: identified(h_flex(), id),
            indent: 0,
            start: None,
            children: Vec::new(),
            end: None,
            selected: false,
            style: ListItemStyle::default(),
        }
    }

    pub(crate) fn indent(mut self, depth: usize) -> Self {
        self.indent = depth;
        self
    }

    pub(crate) fn start(mut self, element: impl IntoElement) -> Self {
        self.start = Some(element.into_any_element());
        self
    }

    pub(crate) fn end(mut self, element: impl IntoElement) -> Self {
        self.end = Some(element.into_any_element());
        self
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub(crate) fn style(mut self, style: ListItemStyle) -> Self {
        self.style = style;
        self
    }
}

impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl InteractiveElement for ListItem {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for ListItem {}

impl RenderOnce for ListItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        let depth = f32::from(u8::try_from(self.indent).unwrap_or(u8::MAX));
        let (inset, radius) = match self.style {
            ListItemStyle::Tree => (theme::ROW_INSET, theme::ROW_RADIUS),
            ListItemStyle::Menu => (theme::ROW_INSET - theme::MENU_INSET, theme::MENU_ROW_RADIUS),
        };
        let row = self
            .base
            // Load-bearing: an `auto` width is clickable only where the text reaches.
            .w_full()
            // A menu row is its line box; a tree row is `TREE_ROW_HEIGHT` tall, the pitch `row_center` reads.
            .ui_text()
            .pl(inset + theme::INDENT_STEP * depth)
            .pr(inset)
            .rounded(radius)
            .map(|row| match self.style {
                ListItemStyle::Tree => row.h(theme::TREE_ROW_HEIGHT),
                // A menu row's icon is not a tree's chevron slot: it keeps off the label.
                ListItemStyle::Menu => row.gap_1p5(),
            });
        let (selected, hover) = match self.style {
            ListItemStyle::Tree => (colors.chrome_element_hover, colors.chrome_element_hover),
            ListItemStyle::Menu => (colors.element_selected, colors.element_hover),
        };
        selectable(row, self.selected.then_some(selected), hover)
            .children(self.start.map(|start| div().flex_none().child(start)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .children(self.children),
            )
            .children(self.end.map(|end| div().flex_none().child(end)))
    }
}
