use std::ops::Range;

use gpui::{
    Context, HighlightStyle, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, StyledText, Window, div,
};

use super::{BacklinksPanel, Occurrence, Row};
use crate::sidebar::NoteActivated;
use crate::theme;

pub(super) fn link_highlight(occurrence: &Occurrence) -> (Range<usize>, HighlightStyle) {
    let style = HighlightStyle {
        color: Some(theme::link_text().into()),
        ..HighlightStyle::default()
    };
    (occurrence.link.clone(), style)
}

impl BacklinksPanel {
    fn render_row(index: usize, row: &Row, cx: &mut Context<Self>) -> impl IntoElement {
        let source = row.source;
        div()
            .debug_selector(|| format!("backlink-{index}"))
            .flex()
            .flex_col()
            .w_full()
            .px_2()
            .pb_2()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| cx.emit(NoteActivated(source))),
            )
            .child(
                div()
                    .text_color(theme::sidebar_text())
                    .child(row.name.clone()),
            )
            .children(row.occurrences.iter().map(|occurrence| {
                div().text_color(theme::placeholder_text()).child(
                    StyledText::new(occurrence.context.clone())
                        .with_highlights([link_highlight(occurrence)]),
                )
            }))
    }
}

impl Render for BacklinksPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let message = match (self.rows.is_empty(), self.building) {
            (true, true) => Some("Indexing…"),
            (true, false) => Some("No backlinks"),
            (false, _) => None,
        };
        div()
            .id("backlinks")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .bg(theme::sidebar_background())
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(theme::placeholder_text())
                    .child("Backlinks"),
            )
            .children(message.map(|message| {
                div()
                    .px_2()
                    .text_color(theme::placeholder_text())
                    .child(message)
            }))
            .children(
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| Self::render_row(index, row, cx).into_any_element())
                    .collect::<Vec<_>>(),
            )
    }
}
