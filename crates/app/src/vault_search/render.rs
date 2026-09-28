use gpui::{
    AnyElement, HighlightStyle, IntoElement, ParentElement, Styled, StyledText, div, prelude::*,
};

use super::rows::{Header, HitRow};
use crate::theme;

/// The name, its folder dimmed, then the hit count; a hairline sets off a later group.
pub(super) fn header(header: &Header, after_group: bool) -> AnyElement {
    div()
        .relative()
        .flex()
        .flex_row()
        .gap_2()
        .w_full()
        .overflow_hidden()
        .child(
            div()
                // Shrinks only once the folder, whose flex basis is 0, has none left to give.
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(header.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis_start()
                .text_color(theme::placeholder_text())
                .child(header.folder.clone()),
        )
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
                    .bg(theme::group_rule()),
            )
        })
        .into_any_element()
}

/// The line number, right-aligned in a column as wide as the largest listed, then the context.
pub(super) fn hit(hit: &HitRow, digits: u8) -> AnyElement {
    let lit = HighlightStyle {
        color: Some(theme::match_text().into()),
        ..HighlightStyle::default()
    };
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
                .child(
                    StyledText::new(hit.context.clone()).with_highlights([(hit.lit.clone(), lit)]),
                ),
        )
        .into_any_element()
}
