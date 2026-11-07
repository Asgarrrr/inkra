//! A tab strip's chrome: the rule under its tabs, its icon slot, and a lower group's header row (splits spec, D3-D4).

use gpui::{App, Div, InteractiveElement, MouseButton, ParentElement, Styled, div};

use super::{h_flex, keep_focus};
use crate::theme;

/// The rule under a strip's tabs, over its row; the active tab, painted later, breaks it.
pub(crate) fn strip_rule(cx: &App) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .border_b(theme::RULE_WIDTH)
        .border_color(theme::current(cx).colors.border_variant)
}

/// A strip's slot for icon buttons, outside the bar's scroll, on the tabs' content centreline.
pub(crate) fn strip_slot() -> Div {
    h_flex()
        .flex_none()
        .self_end()
        .h(theme::TAB_HEIGHT)
        .pt(theme::RULE_WIDTH)
        .pb(theme::TAB_HOVER_INSET)
}

/// A lower group's header row, as the title row holds only the top groups' strips (splits spec, D4).
pub(crate) fn lower_header(cx: &App) -> Div {
    h_flex()
        .flex_none()
        .relative()
        .h(theme::GROUP_HEADER_HEIGHT)
        .bg(theme::current(cx).colors.background)
        .on_mouse_down(MouseButton::Left, keep_focus)
        .child(strip_rule(cx))
}
