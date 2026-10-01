use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, Stateful, StatefulInteractiveElement, Styled, Window, div,
};

use super::{UiText, h_flex};
use crate::theme;

/// A row of a list. It owns background, hover and padding; a caller attaches handlers, role,
/// labels and content, and a caller's `.hover()` trips gpui's "hover style already set" assert.
#[derive(IntoElement)]
pub(crate) struct ListItem {
    base: Stateful<Div>,
    indent: usize,
    start: Option<AnyElement>,
    children: Vec<AnyElement>,
    selected: bool,
}

impl ListItem {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: h_flex().debug_selector(|| id.to_string()).id(id),
            indent: 0,
            start: None,
            children: Vec::new(),
            selected: false,
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

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
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
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let depth = f32::from(u8::try_from(self.indent).unwrap_or(u8::MAX));
        let row = self
            .base
            // Load-bearing: an `auto` width is clickable only where the text reaches.
            .w_full()
            // No height: the row is its line box (`a_click_far_down_the_list_lands_on_the_row_it_names`).
            .ui_text()
            .pl(theme::ROW_INSET + theme::INDENT_STEP * depth)
            .pr(theme::ROW_INSET);
        // A hover style would repaint a selected row too (`div.rs:3451`), so a row carries one or the other.
        let row = if self.selected {
            row.bg(theme::selected_row_background())
        } else {
            row.hover(|style| style.bg(theme::row_hover_background()))
        };
        row.children(self.start.map(|start| div().flex_none().child(start)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .children(self.children),
            )
    }
}
