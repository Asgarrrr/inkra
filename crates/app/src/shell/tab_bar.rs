//! The row of tabs above the note column (tabs spec, A5).

use std::cmp::Ordering;

use gpui::{
    App, AppContext, ClickEvent, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Window, div, prelude::FluentBuilder,
};
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use super::AppShell;
use crate::one_line::one_line;
use crate::theme;
use crate::vault::NoteStore;

const NEW_TAB: &str = "New tab";
const CLOSE: &str = "×";

/// The tab under the pointer; also its preview. `index` is where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedTab {
    id: TabId,
    index: usize,
    title: SharedString,
    active: bool,
}

impl Render for DraggedTab {
    /// Drawn at the window root, so it inherits nothing from the bar and states its whole look.
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .max_w(theme::TAB_MAX_WIDTH)
            .truncate()
            .bg(theme::tab_active_background())
            .text_color(if self.active {
                theme::tab_text()
            } else {
                theme::tab_inactive_text()
            })
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .child(self.title.clone())
    }
}

fn tab_title(note: Option<EntryId>, notes: &NoteStore, cx: &App) -> SharedString {
    note.and_then(|id| notes.title(id, cx))
        .map_or(SharedString::new_static(NEW_TAB), |title| {
            one_line(&title).into_owned().into()
        })
}

impl AppShell {
    /// Every tab's title, reading the store once.
    pub(super) fn tab_titles(&self, cx: &App) -> Vec<SharedString> {
        let (notes, showing) = (self.notes.read(cx), self.showing());
        self.tabs
            .iter()
            .map(|tab| tab_title(tab.place().map_or(showing, |place| place.note), notes, cx))
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
            .map(|(ix, (tab, title))| Self::render_tab(ix, tab.id(), title, tab.id() == active, cx))
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

    fn render_tab(
        ix: usize,
        id: TabId,
        title: SharedString,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let dragged = DraggedTab {
            id,
            index: ix,
            title: title.clone(),
            active: is_active,
        };
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
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press_tab))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::press_tab))
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
            .on_drag(dragged, |tab, _, _, cx| cx.new(|_| tab.clone()))
            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                let style = style
                    .bg(theme::tab_drop_background())
                    .border_color(theme::tab_drop_border());
                match ix.cmp(&dragged.index) {
                    Ordering::Less => style.border_l(theme::TAB_DROP_EDGE),
                    Ordering::Greater => style.border_r(theme::TAB_DROP_EDGE),
                    Ordering::Equal => style,
                }
            })
            .on_drop(cx.listener(move |shell, dragged: &DraggedTab, _, cx| {
                shell.move_tab(dragged.id, ix, cx);
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
    }

    /// Every press on a tab lands focus on the active tab's content: gpui's root focus-on-press
    /// (div.rs:2765-2780) is prevented, and a click on the active tab, a drop or an inactive
    /// close shows no place that would focus it.
    fn press_tab(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.prevent_default();
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }
}
