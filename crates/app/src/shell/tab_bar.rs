//! The row of tabs above the note column (tabs spec, A5).

use gpui::{
    App, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};

use super::AppShell;
use super::tabs::Place;
use crate::one_line::one_line;
use crate::theme;
use crate::vault::NoteStore;

const NEW_TAB: &str = "New tab";
const CLOSE: &str = "×";

fn tab_title(place: &Place, notes: &NoteStore, cx: &App) -> String {
    place
        .note
        .and_then(|id| notes.title(id, cx))
        .map_or_else(|| NEW_TAB.to_owned(), |title| one_line(&title).into_owned())
}

impl AppShell {
    /// Every tab's title, reading the store once.
    pub(super) fn tab_titles(&self, cx: &App) -> Vec<String> {
        let notes = self.notes.read(cx);
        self.tabs
            .iter()
            .map(|tab| tab_title(tab.place(), notes, cx))
            .collect()
    }

    /// Its children are the tabs only, so `scroll_to_item` indexes tabs (`div.rs:4291-4322`).
    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active();
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .zip(self.tab_titles(cx))
            .enumerate()
            .map(|(ix, (tab, title))| {
                let (id, is_active) = (tab.id(), tab.id() == active);
                div()
                    .id(("tab", ix))
                    .debug_selector(move || format!("tab-{ix}"))
                    .flex()
                    .flex_row()
                    .flex_none()
                    .gap_2()
                    .max_w(theme::TAB_MAX_WIDTH)
                    .px_2()
                    .when(is_active, |tab| tab.bg(theme::tab_active_background()))
                    .text_color(if is_active {
                        theme::tab_text()
                    } else {
                        theme::tab_inactive_text()
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.activate_tab(id, window, cx);
                    }))
                    // The × has no aux listener, so a middle click on it closes its tab here.
                    .on_aux_click(cx.listener(move |shell, event: &ClickEvent, window, cx| {
                        if event.is_middle_click() {
                            cx.stop_propagation();
                            shell.close_tab(id, window, cx);
                        }
                    }))
                    .child(div().min_w_0().truncate().child(title))
                    .child(
                        div()
                            .id(("tab-close", ix))
                            .debug_selector(move || format!("tab-close-{ix}"))
                            .flex_none()
                            .child(CLOSE)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                // One click, one action: the tab under the × must not also see it.
                                cx.stop_propagation();
                                shell.close_tab(id, window, cx);
                            })),
                    )
            })
            .collect();
        div()
            .id("tab-bar")
            .debug_selector(|| "tab-bar".into())
            .flex()
            .flex_row()
            .flex_none()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .bg(theme::tab_bar_background())
            .border_b(theme::RULE_WIDTH)
            .border_color(theme::tab_rule())
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .children(tabs)
    }
}
