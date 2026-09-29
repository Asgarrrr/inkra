use std::ops::Range;

#[cfg(any(test, feature = "shot"))]
use gpui::Pixels;
use gpui::{
    App, Context, Div, InteractiveElement, MouseButton, ParentElement, Styled, Window, div,
};

use crate::one_line::one_line;
use crate::theme;

use super::rows::Item;
use super::{EditState, INDENT_BASE, INDENT_STEP, Row, Sidebar};

impl Sidebar {
    /// Split out only so a test can read the dot.
    pub(super) fn row_label(&self, row: &Row, cx: &App) -> String {
        let dot = if self.notes.read(cx).is_dirty(row.id, cx) {
            " •"
        } else {
            ""
        };
        format!("{}{dot}", one_line(&row.name))
    }

    /// The field's row: same box as any other, or a pixel taller misplaces every row below it.
    fn render_field(edit: &EditState) -> Div {
        let depth = f32::from(u8::try_from(edit.depth).unwrap_or(u8::MAX));
        div()
            .flex()
            .flex_row()
            .w_full()
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .pl(INDENT_BASE + INDENT_STEP * depth)
            .bg(theme::selected_row_background())
            .child(edit.field.clone())
    }

    pub(super) fn render_rows(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        range
            .map(|item| match self.rows.item(item) {
                Some(Item::Row(row)) => self.render_row(&self.rows.visible().row(row), cx),
                Some(Item::Field(edit)) => Self::render_field(edit),
                None => div(),
            })
            .collect()
    }

    fn render_row(&self, row: &Row, cx: &mut Context<Self>) -> Div {
        let row = row.clone();
        {
            // Reading inside the draw is what repaints the dot; `Entity::cached` would break it.
            let label = self.row_label(&row, cx);
            let depth = f32::from(u8::try_from(row.depth).unwrap_or(u8::MAX));
            let background = if self.selected == Some(row.id) {
                theme::selected_row_background()
            } else {
                theme::sidebar_background()
            };
            div()
                .flex()
                .flex_row()
                // Load-bearing: an `auto` width is clickable only where the text reaches.
                .w_full()
                // No explicit height: the row is its line box. Delete this and
                // `a_click_far_down_the_list_lands_on_the_row_it_names` fails.
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .pl(INDENT_BASE + INDENT_STEP * depth)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(theme::sidebar_text())
                .bg(background)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |sidebar, _event, window, cx| {
                        // Focus first, so the tree's bindings resolve from the next keystroke.
                        window.focus(&sidebar.focus_handle, cx);
                        sidebar.activate(&row, cx);
                    }),
                )
                .child(label)
        }
    }
}

/// Not `#[cfg(test)]`-gated: the shot runner needs it in a normal build too.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn row_center(ix: f32) -> Pixels {
    theme::UI_LINE_HEIGHT * (ix + 0.5)
}
