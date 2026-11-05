use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};

use super::{UiText, h_flex, identified, selectable};
use crate::theme;

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
    inset: bool,
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
            inset: false,
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

    /// The tree row's shape: `TREE_ROW_HEIGHT` tall, rounded, lit with the chrome's row colour.
    pub(crate) fn inset(mut self) -> Self {
        self.inset = true;
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
        let row = self
            .base
            // Load-bearing: an `auto` width is clickable only where the text reaches.
            .w_full()
            // A non-inset row is its line box; an inset row is `TREE_ROW_HEIGHT` tall, the pitch `row_center` reads.
            .ui_text()
            .pl(theme::ROW_INSET + theme::INDENT_STEP * depth)
            .pr(theme::ROW_INSET)
            .when(self.inset, |row| {
                row.h(theme::TREE_ROW_HEIGHT).rounded(theme::ROW_RADIUS)
            });
        let (selected, hover) = if self.inset {
            (colors.chrome_element_hover, colors.chrome_element_hover)
        } else {
            (colors.element_selected, colors.element_hover)
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
