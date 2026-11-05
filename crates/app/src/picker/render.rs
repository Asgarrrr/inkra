use std::ops::Range;

use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, KeyContext, ParentElement,
    Render, Role, StatefulInteractiveElement, Styled, Window, div, uniform_list,
};

use super::{Picker, PickerDelegate};
use crate::open::Open;
use crate::theme;
use crate::ui::{Color, Label, ListItem, UiText};

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
                ListItem::new(("picker-row", ix))
                    .role(Role::ListItem)
                    .aria_label(self.delegate.row_label(ix))
                    .selected(selected)
                    .on_click(cx.listener(move |picker, event: &ClickEvent, _, cx| {
                        // The release's modifiers: a picker row acts at mouse-up, a sidebar row at the press.
                        picker.click(ix, Open::from_click(event.modifiers()), cx);
                    }))
                    .child(
                        div()
                            .whitespace_nowrap()
                            .child(self.delegate.render_row(ix, selected, cx)),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(feature = "shot")]
impl<D: PickerDelegate> Picker<D> {
    /// Where row `ix` was drawn last, for a scene to point at; `None` before the list is drawn.
    pub(crate) fn row_center(&self, ix: f32) -> Option<gpui::Point<gpui::Pixels>> {
        crate::ui::list_row_center(&self.scroll, self.delegate.row_height(), ix)
    }
}

impl<D: PickerDelegate> Render for Picker<D> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut context = KeyContext::default();
        context.add("Picker");
        context.add(self.delegate.key_context());
        let theme = theme::current(cx);
        let (text, placeholder) = (Color::Default.rgba(theme), Color::Placeholder.rgba(theme));
        let count = self.delegate.match_count();
        let shown = u8::try_from(count).unwrap_or(u8::MAX).min(MAX_ROWS);
        let body = if count == 0 {
            div()
                .px(theme::ROW_INSET)
                .children(
                    self.delegate
                        .empty_text(cx)
                        .map(|empty| Label::new(empty).color(Color::Placeholder)),
                )
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
                .px(theme::ROW_INSET)
                .text_color(placeholder)
                .child(footer)
        });
        div()
            .id("picker")
            .role(Role::Dialog)
            .aria_label(self.delegate.label())
            .key_context(context)
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::secondary_confirm))
            .flex()
            .flex_col()
            .w_full()
            .py_1()
            .ui_text()
            .text_color(text)
            .child(
                div()
                    .debug_selector(|| "picker-field".into())
                    .relative()
                    .px(theme::ROW_INSET)
                    .pb_1()
                    .child(self.field.clone())
                    .children(self.query.is_empty().then(|| {
                        div().absolute().inset_0().px(theme::ROW_INSET).child(
                            Label::new(self.delegate.placeholder()).color(Color::Placeholder),
                        )
                    })),
            )
            .child(body)
            .children(footer)
    }
}
