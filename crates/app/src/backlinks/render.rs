use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div,
};

use super::{BacklinksPanel, Row};
use crate::open::{NoteActivated, Open};
use crate::row_text::styled;
use crate::theme;

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
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    // The press's modifiers: the row acts at mouse-down, a picker row at the release.
                    cx.emit(NoteActivated {
                        note: source,
                        open: Open::from_click(event.modifiers),
                    });
                }),
            )
            .child(
                div()
                    .text_color(theme::sidebar_text())
                    .child(row.name.clone()),
            )
            .children(row.occurrences.iter().map(|occurrence| {
                div()
                    .text_color(theme::placeholder_text())
                    .child(styled(&occurrence.context, theme::link_text().into()))
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
