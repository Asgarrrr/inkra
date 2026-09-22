use std::ops::Range;

#[cfg(any(test, feature = "shot"))]
use gpui::Pixels;
use gpui::{
    App, Context, Div, InteractiveElement, MouseButton, ParentElement, Styled, Window, div,
};

use crate::theme;

use super::{EditState, INDENT_BASE, INDENT_STEP, Row, Sidebar};

impl Sidebar {
    /// Split out only so a test can read the dot: nothing gets text back out
    /// of the `Div`s `render_rows` returns.
    pub(super) fn row_label(&self, row: &Row, cx: &App) -> String {
        let dot = if self.notes.read(cx).is_dirty(row.id, cx) {
            " •"
        } else {
            ""
        };
        format!("{}{dot}", row.name)
    }

    /// The field's row: the same box as any other, so the pitch does not move.
    /// `uniform_list` measures every row from item 0
    /// (`elements/uniform_list.rs:658-680`), so a field even a pixel taller
    /// would misplace every row below it.
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
            .map(|ix| {
                let Some(row) = self.row_at(ix) else {
                    // Only the field's index yields `None`, and only while one
                    // is open, so this cannot be reached without an `EditState`.
                    return self.editing.as_ref().map_or_else(div, Self::render_field);
                };
                self.render_row(&row, cx)
            })
            .collect()
    }

    /// `cx.processor` re-enters the sidebar entity in `render_rows`
    /// (`gpui/src/app/context.rs:260-267`); that is the only reason
    /// `cx.listener` is reachable from `uniform_list`'s `&mut App` closure.
    fn render_row(&self, row: &Row, cx: &mut Context<Self>) -> Div {
        let row = row.clone();
        {
            // Reading inside the draw is what repaints the dot; no
            // `cx.observe` needed and `Entity::cached` would break it
            // (`app.rs:2789-2823`, `view.rs:488`).
            let label = self.row_label(&row, cx);
            // `Vault::scan` stops at depth 64, so nothing saturates.
            let depth = f32::from(u8::try_from(row.depth).unwrap_or(u8::MAX));
            // `FluentBuilder::when` would read better, but gpui keeps that
            // trait private in this build (`gpui.rs:61`, `:163`).
            let background = if self.selected == Some(row.id) {
                theme::selected_row_background()
            } else {
                theme::sidebar_background()
            };
            div()
                .flex()
                .flex_row()
                // Load-bearing: an `auto` width is clickable only where the
                // text reaches (`elements/uniform_list.rs:508-510`).
                .w_full()
                // No explicit height: the row *is* its line box, so it
                // cannot clip its own text however the type is set (see
                // CLAUDE.md on the chrome scale). That identity also ties the
                // row pitch to this statement — delete it and
                // `a_click_far_down_the_list_lands_on_the_row_it_names` fails.
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .pl(INDENT_BASE + INDENT_STEP * depth)
                // Load-bearing: row height is measured from item 0 alone
                // (`elements/uniform_list.rs:658-680`), so a wrapping
                // filename overflows its row.
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(theme::sidebar_text())
                .bg(background)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |sidebar, _event, window, cx| {
                        // Focus first, so the tree's bindings resolve from
                        // the next keystroke — straight to the handle, as
                        // `EditorView::on_mouse_down` does.
                        window.focus(&sidebar.focus_handle, cx);
                        sidebar.activate(&row, cx);
                    }),
                )
                .child(label)
        }
    }
}

/// The vertical centre of row `ix`, for a test that clicks one. Derived from
/// the line height, since the row has no height of its own (see
/// `render_rows`). Shared with `shell.rs` and the shot runner so neither
/// hard-codes the pitch; not `#[cfg(test)]`-gated because the shot runner
/// needs it in a normal build.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn row_center(ix: f32) -> Pixels {
    theme::UI_LINE_HEIGHT * (ix + 0.5)
}
