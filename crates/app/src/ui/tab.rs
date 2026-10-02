use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, Role, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};

use super::{UiText, h_flex, identified, selectable};
use crate::theme;

/// A tab's end slot reads this group to show on the tab's hover; gpui resolves it to the nearest tab.
const TAB_GROUP: &str = "tab";

/// One tab of the bar, and its drag preview. It owns the look; the bar attaches the handlers.
#[derive(IntoElement)]
pub(crate) struct Tab {
    base: Stateful<Div>,
    active: bool,
    start: Option<AnyElement>,
    end: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl Tab {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: identified(h_flex(), id),
            active: false,
            start: None,
            end: None,
            children: Vec::new(),
        }
    }

    pub(crate) fn active(mut self, active: bool) -> Self {
        self.active = active;
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
}

impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Tab {}

impl RenderOnce for Tab {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let active = self.active;
        let tab = self
            .base
            .group(TAB_GROUP)
            .role(Role::Tab)
            .aria_selected(self.active)
            .flex_none()
            .gap_2()
            .max_w(theme::TAB_MAX_WIDTH)
            .px(theme::ROW_INSET)
            .ui_text()
            .text_color(if self.active {
                theme::tab_text()
            } else {
                theme::tab_inactive_text()
            });
        selectable(tab, self.active.then(theme::tab_active_background))
            .children(self.start.map(|start| div().flex_none().child(start)))
            .child(
                div()
                    .min_w(theme::TAB_MIN_WIDTH)
                    .truncate()
                    .children(self.children),
            )
            .children(self.end.map(|end| {
                div()
                    .flex_none()
                    // Opacity, not `invisible()`: a hidden div registers no listener (`div.rs:2529`).
                    .when(!active, |end| {
                        end.opacity(0.)
                            .group_hover(TAB_GROUP, |style| style.opacity(1.))
                    })
                    .child(end)
            }))
    }
}
