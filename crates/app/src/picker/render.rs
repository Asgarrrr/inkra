use std::ops::Range;

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyContext, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, Window, div, prelude::*, uniform_list,
};

use super::{Picker, PickerDelegate};
use crate::theme;

/// Past this the list scrolls.
const MAX_ROWS: u8 = 10;

impl<D: PickerDelegate> Picker<D> {
    fn render_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.delegate.rows_shown(range.clone(), cx);
        range
            .map(|ix| {
                let selected = self.selected == Some(ix);
                div()
                    .id(("picker-row", ix))
                    .role(Role::ListItem)
                    .aria_label(self.delegate.row_label(ix))
                    .debug_selector(|| format!("picker-row-{ix}"))
                    // Load-bearing: an `auto` width is clickable only where the text reaches.
                    .w_full()
                    .px_2()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .when(selected, |row| row.bg(theme::selected_row_background()))
                    .on_click(cx.listener(move |picker, _, _, cx| picker.click(ix, cx)))
                    .child(self.delegate.render_row(ix, selected, cx))
                    .into_any_element()
            })
            .collect()
    }
}

impl<D: PickerDelegate> Render for Picker<D> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut context = KeyContext::default();
        context.add("Picker");
        context.add(self.delegate.key_context());
        let count = self.delegate.match_count();
        let shown = u8::try_from(count).unwrap_or(u8::MAX).min(MAX_ROWS);
        let body = if count == 0 {
            div()
                .px_2()
                .text_color(theme::placeholder_text())
                .children(self.delegate.empty_text())
                .into_any_element()
        } else {
            div()
                .id("picker-list")
                .role(Role::List)
                .debug_selector(|| "picker-list".into())
                .w_full()
                .child(
                    uniform_list("picker-rows", count, cx.processor(Self::render_rows))
                        .w_full()
                        .h(self.delegate.row_height() * f32::from(shown))
                        .track_scroll(&self.scroll),
                )
                .into_any_element()
        };
        let footer = self.delegate.render_footer(cx).map(|footer| {
            div()
                .debug_selector(|| "picker-footer".into())
                .px_2()
                .text_color(theme::placeholder_text())
                .child(footer)
        });
        div()
            .id("picker")
            .role(Role::Dialog)
            .aria_label(self.delegate.label())
            .key_context(context)
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .flex()
            .flex_col()
            .w_full()
            .py_1()
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(theme::sidebar_text())
            .child(
                div()
                    .debug_selector(|| "picker-field".into())
                    .relative()
                    .px_2()
                    .pb_1()
                    .child(self.field.clone())
                    .children(self.query.is_empty().then(|| {
                        div()
                            .absolute()
                            .inset_0()
                            .px_2()
                            .text_color(theme::placeholder_text())
                            .child(self.delegate.placeholder())
                    })),
            )
            .child(body)
            .children(footer)
    }
}
