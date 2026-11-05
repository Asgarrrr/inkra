use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div,
};

use super::{BacklinksPanel, Row};
use crate::open::{NoteActivated, Open};
use crate::row_text::styled;
use crate::theme;
use crate::ui::{Color, Label, ListItem, UiText, v_flex};

impl BacklinksPanel {
    fn render_row(index: usize, row: &Row, cx: &mut Context<Self>) -> impl IntoElement {
        let source = row.source;
        let theme = theme::current(cx);
        let (name, context, link) = (
            Color::Default.rgba(theme),
            Color::Placeholder.rgba(theme),
            Color::Accent.rgba(theme),
        );
        ListItem::new(("backlink", index))
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
                v_flex()
                    .pb_2()
                    .child(div().text_color(name).child(row.name.clone()))
                    .children(row.occurrences.iter().map(|occurrence| {
                        div()
                            .text_color(context)
                            .child(styled(&occurrence.context, link.into()))
                    })),
            )
    }
}

impl Render for BacklinksPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let message = match (self.rows.is_empty(), self.building) {
            (true, true) => Some("Indexing…"),
            (true, false) => Some("No backlinks"),
            (false, _) => None,
        };
        v_flex()
            .id("backlinks")
            .size_full()
            .ui_text()
            .overflow_y_scroll()
            .children(message.map(|message| {
                div()
                    .px(theme::ROW_INSET)
                    .child(Label::new(message).color(Color::Placeholder))
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
