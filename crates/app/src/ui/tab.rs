use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    RenderOnce, Role, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};

use super::{Color, UiText, h_flex, identified};
use crate::theme::{self, ThemeColors};

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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = theme::current(cx);
        let colors = theme.colors;
        let active = self.active;
        let tab = self
            .base
            .group(TAB_GROUP)
            .role(Role::Tab)
            .aria_selected(self.active)
            .relative()
            .flex_none()
            .h(theme::TAB_HEIGHT)
            .rounded_t(theme::TAB_RADIUS)
            .gap_2()
            .max_w(theme::TAB_MAX_WIDTH)
            // The content centres on the hover pill, as the reference's.
            .pt(theme::RULE_WIDTH)
            .pb(theme::TAB_HOVER_INSET)
            .pl(theme::TAB_TEXT_INSET)
            .pr(theme::ROW_INSET)
            .ui_text()
            .text_color(
                if self.active {
                    Color::Default
                } else {
                    Color::Muted
                }
                .rgba(theme),
            );
        tab.when(active, |tab| tab.bg(colors.editor_background))
            .when(!active, |tab| tab.child(hover_pill(&colors)))
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
            .when(active, |tab| {
                // Paint-only: a layout border would shift the title on activation and take the drop indicator's colour.
                tab.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        // The feet's curves carry the sides down to the row's bottom.
                        .bottom(theme::TAB_CURVE)
                        .rounded_t(theme::TAB_RADIUS)
                        .border_t(theme::RULE_WIDTH)
                        .border_l(theme::RULE_WIDTH)
                        .border_r(theme::RULE_WIDTH)
                        .border_color(colors.border_variant),
                )
                .child(foot(Side::Left, &colors))
                .child(foot(Side::Right, &colors))
            })
    }
}

/// An inactive tab's hover: a pill inset in the tab, clear of the row's bottom (the reference's tab padding `1px 4px 4px`).
fn hover_pill(colors: &ThemeColors) -> impl IntoElement {
    div()
        .debug_selector(|| "tab-hover".into())
        .absolute()
        .top(theme::RULE_WIDTH)
        .left(theme::TAB_HOVER_INSET)
        .right(theme::TAB_HOVER_INSET)
        .bottom(theme::TAB_HOVER_INSET)
        .rounded(theme::TAB_HOVER_RADIUS)
        .group_hover(TAB_GROUP, |style| style.bg(colors.chrome_element_hover))
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// The active tab's concave foot outside one bottom corner, joining it to the note (the reference's `--tab-curve`).
/// A ring in the note's colour, clipped to the corner's square, leaves the chrome showing through its hole.
fn foot(side: Side, colors: &ThemeColors) -> impl IntoElement {
    let curve = theme::TAB_CURVE;
    let rule = theme::RULE_WIDTH;
    // The square overlaps the tab's edge by the rule, so the curve's hairline meets the side's hairline.
    let (square, centre) = match side {
        Side::Left => (div().left(-curve), rule),
        Side::Right => (div().right(-curve), curve),
    };
    square
        .absolute()
        .bottom_0()
        .w(curve + rule)
        .h(curve)
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(centre - curve * 2.)
                .top(-curve * 2.)
                .size(curve * 4.)
                .rounded_full()
                .border(curve)
                .border_color(colors.editor_background),
        )
        .child(
            div()
                .absolute()
                .left(centre - curve)
                .top(-curve)
                .size(curve * 2.)
                .rounded_full()
                .border(rule)
                .border_color(colors.border_variant),
        )
}
