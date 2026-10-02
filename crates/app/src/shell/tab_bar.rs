//! The row of tabs above the note column (tabs spec, A5).

use std::cmp::Ordering;

use gpui::{
    App, AppContext, ClickEvent, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Window, div, prelude::FluentBuilder,
};
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use super::AppShell;
use crate::one_line::one_line;
use crate::theme;
use crate::ui::{Icon, IconName, Indicator, Tab, UiText};
use crate::vault::NoteStore;

const NEW_TAB: &str = "New tab";

/// The tab under the pointer; also its preview. `index` is where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedTab {
    id: TabId,
    index: usize,
    title: SharedString,
    active: bool,
}

impl Render for DraggedTab {
    /// Drawn at the window root, so it inherits nothing; an inactive `Tab` has no background, so this paints one.
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().bg(theme::tab_active_background()).child(
            Tab::new("tab-preview")
                .active(self.active)
                .child(self.title.clone()),
        )
    }
}

fn tab_title(note: Option<EntryId>, notes: &NoteStore, cx: &App) -> SharedString {
    note.and_then(|id| notes.title(id, cx))
        .map_or(SharedString::new_static(NEW_TAB), |title| {
            one_line(&title).into_owned().into()
        })
}

impl AppShell {
    /// The note each tab shows; `None` for an empty tab.
    pub(super) fn tab_notes(&self) -> impl Iterator<Item = Option<EntryId>> + '_ {
        let active = self.active_note();
        self.tabs
            .iter()
            .map(move |tab| tab.place().map_or(active, inkra_tabs::Place::note))
    }

    /// Every tab's title, reading the store once.
    pub(super) fn tab_titles(&self, cx: &App) -> Vec<SharedString> {
        let notes = self.notes.read(cx);
        self.tab_notes()
            .map(|note| tab_title(note, notes, cx))
            .collect()
    }

    /// Its children are the tabs only, so `scroll_to_item` indexes tabs (`div.rs:4291-4322`).
    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tabs.active();
        let notes = self.notes.read(cx);
        // Reading inside the draw is what repaints the dot; `Entity::cached` would break it.
        let dirty: Vec<bool> = self
            .tab_notes()
            .map(|note| note.is_some_and(|id| notes.is_dirty(id, cx)))
            .collect();
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .zip(self.tab_titles(cx))
            .zip(dirty)
            .enumerate()
            .map(|(ix, ((tab, title), dirty))| {
                Self::render_tab(ix, tab.id(), title, tab.id() == active, dirty, cx)
            })
            .collect();
        div()
            .id("tab-bar")
            .debug_selector(|| "tab-bar".into())
            .role(Role::TabList)
            .flex()
            .flex_row()
            .flex_none()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .bg(theme::tab_bar_background())
            .border_b(theme::RULE_WIDTH)
            .border_color(theme::tab_rule())
            .ui_text()
            .children(tabs)
    }

    fn render_tab(
        ix: usize,
        id: TabId,
        title: SharedString,
        is_active: bool,
        dirty: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let dragged = DraggedTab {
            id,
            index: ix,
            title: title.clone(),
            active: is_active,
        };
        Tab::new(("tab", ix))
            .active(is_active)
            .when(dirty, |tab| {
                tab.start(
                    div()
                        .debug_selector(move || format!("tab-dot-{ix}"))
                        .child(Indicator),
                )
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
            .child(title)
            .end(
                div()
                    .id(("tab-close", ix))
                    .debug_selector(move || format!("tab-close-{ix}"))
                    .p_0p5()
                    .rounded_sm()
                    .hover(|style| style.bg(theme::button_hover_background()))
                    .role(Role::Button)
                    .aria_label("Close tab")
                    .child(Icon::new(IconName::X))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // One click, one action: the tab under the × must not also see it.
                        cx.stop_propagation();
                        shell.close_tab(id, window, cx);
                    })),
            )
    }

    /// Focuses the active tab's content: gpui's root focus-on-press (div.rs:2765-2780) is prevented.
    fn press_tab(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.prevent_default();
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }
}
