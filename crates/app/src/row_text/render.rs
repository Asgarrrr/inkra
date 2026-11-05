use std::ops::Range;

use gpui::{Div, HighlightStyle, Hsla, InteractiveElement, ParentElement, Styled, StyledText, div};

use super::{LitText, PathText};
use crate::theme::Theme;
use crate::ui::Color;

pub(super) fn highlights(
    lit: &LitText,
    color: Hsla,
) -> impl Iterator<Item = (Range<usize>, HighlightStyle)> + '_ {
    let style = HighlightStyle {
        color: Some(color),
        ..HighlightStyle::default()
    };
    lit.ranges().iter().map(move |range| (range.clone(), style))
}

pub(crate) fn styled(lit: &LitText, color: Hsla) -> StyledText {
    StyledText::new(lit.text().clone()).with_highlights(highlights(lit, color))
}

/// The name, then the folder dimmed; a caller appends children (the hit count).
pub(crate) fn name_row(path: &PathText, theme: &Theme) -> Div {
    let color = Color::Highlight.rgba(theme).into();
    div()
        .flex()
        .flex_row()
        .gap_2()
        .w_full()
        .overflow_hidden()
        .child(
            div()
                .debug_selector(|| "row-name".into())
                // Shrinks only once the folder, whose flex basis is 0, has none left to give.
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(styled(&path.name, color)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                // Cut from the start, so the nearest folder stays visible.
                .text_ellipsis_start()
                .text_color(Color::Placeholder.rgba(theme))
                .child(styled(&path.folder, color)),
        )
}
