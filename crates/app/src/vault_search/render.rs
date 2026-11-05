use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, div, prelude::*};

use super::rows::{Header, HitRow};
use crate::row_text::{name_row, styled};
use crate::theme;

/// The name, its folder dimmed, then the hit count; a hairline sets off a later group.
pub(super) fn header(header: &Header, after_group: bool, cx: &App) -> AnyElement {
    let colors = theme::current(cx).colors;
    name_row(&header.path, theme::match_text().into())
        .relative()
        .child(
            div()
                .flex_none()
                .text_color(theme::placeholder_text())
                .child(header.count.clone()),
        )
        // Absolute, so the row keeps the one height `uniform_list` measures.
        .when(after_group, |row| {
            row.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(theme::RULE_WIDTH)
                    .bg(colors.border),
            )
        })
        .into_any_element()
}

/// The line number, right-aligned in a column as wide as the largest listed, then the context.
pub(super) fn hit(hit: &HitRow, digits: u8) -> AnyElement {
    let column = theme::ui_digit_width() * f32::from(digits);
    div()
        .flex()
        .flex_row()
        .gap_2()
        .w_full()
        // Under its header, whatever the width of the line column.
        .pl_4()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .w(column)
                .flex()
                .justify_end()
                .text_color(theme::placeholder_text())
                .child(hit.line.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(styled(&hit.context, theme::match_text().into())),
        )
        .into_any_element()
}
