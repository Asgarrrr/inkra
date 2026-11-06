//! The tabs, in the title row over every column (tabs spec, A5; shell chrome spec, S3).

use std::cmp::Ordering;

use gpui::{
    App, AppContext, ClickEvent, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    Window, div, prelude::FluentBuilder,
};
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use super::{AppShell, NewTab};
use crate::one_line::one_line;
use crate::theme;
use crate::ui::{IconButton, IconName, Indicator, Tab, UiText, h_flex};
use crate::vault::NoteStore;

const NEW_TAB: &str = "New tab";

/// The tab under the pointer; also its preview. `index` is where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedTab {
    id: TabId,
    index: usize,
    title: SharedString,
}

impl Render for DraggedTab {
    /// Drawn at the window root, so it inherits nothing.
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Always the active look: an inactive tab has no background to float on.
        Tab::new("tab-preview")
            .active(true)
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
    /// The note each tab shows; `None` for an empty tab.
    pub(super) fn tab_notes(&self) -> impl Iterator<Item = Option<EntryId>> + '_ {
        let active = self.active_note();
        self.center()
            .tabs
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
        let active = self.center().tabs.active();
        let notes = self.notes.read(cx);
        // Reading inside the draw is what repaints the dot; `Entity::cached` would break it.
        let indicators: Vec<Option<Indicator>> = self
            .tab_notes()
            .map(|note| {
                note.and_then(|id| Indicator::of(notes.is_dirty(id, cx), notes.save_failed(id)))
            })
            .collect();
        let tabs: Vec<_> = self
            .center()
            .tabs
            .iter()
            .zip(self.tab_titles(cx))
            .zip(indicators)
            .enumerate()
            .map(|(ix, ((tab, title), indicator))| {
                Self::render_tab(ix, tab.id(), title, tab.id() == active, indicator, cx)
            })
            .collect();
        div()
            .id("tab-bar")
            .debug_selector(|| "tab-bar".into())
            .role(Role::TabList)
            .flex()
            .flex_row()
            // As wide as its tabs, so the row's space after them is the drag region's.
            .flex_initial()
            .min_w_0()
            .h_full()
            .items_end()
            // Room for the active tab's feet, which the scroll would clip.
            .px(theme::TAB_CURVE)
            .gap(theme::TAB_GAP)
            .overflow_x_scroll()
            .track_scroll(&self.center().tab_scroll)
            .ui_text()
            .children(tabs)
    }

    fn render_tab(
        ix: usize,
        id: TabId,
        title: SharedString,
        is_active: bool,
        indicator: Option<Indicator>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = theme::current(cx).colors;
        let dragged = DraggedTab {
            id,
            index: ix,
            title: title.clone(),
        };
        Tab::new(("tab", ix))
            .active(is_active)
            .when_some(indicator, |tab, indicator| {
                tab.start(
                    div()
                        .debug_selector(move || indicator.selector("tab", ix))
                        .child(indicator),
                )
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press_tab))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::press_tab))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.activate_tab(shell.layout.center(), id, window, cx);
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
                    .bg(colors.accent_background)
                    .border_color(colors.accent);
                match ix.cmp(&dragged.index) {
                    Ordering::Less => style.border_l(theme::DROP_EDGE),
                    Ordering::Greater => style.border_r(theme::DROP_EDGE),
                    Ordering::Equal => style,
                }
            })
            .on_drop(cx.listener(move |shell, dragged: &DraggedTab, _, cx| {
                shell.move_tab(dragged.id, ix, cx);
            }))
            .child(title)
            .end(
                IconButton::new(("tab-close", ix), IconName::X, "Close tab")
                    .square(theme::TAB_CLOSE_SIZE)
                    .radius(theme::TAB_CLOSE_RADIUS)
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // One click, one action: the tab under the × must not also see it.
                        cx.stop_propagation();
                        shell.close_tab(id, window, cx);
                    })),
            )
    }

    /// After the bar, not in it: `scroll_to_item` indexes the bar's children, which are tabs only.
    pub(super) fn render_new_tab(cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .self_end()
            .h(theme::TAB_HEIGHT)
            // On the tabs' content centreline.
            .pt(theme::RULE_WIDTH)
            .pb(theme::TAB_HOVER_INSET)
            .child(
                IconButton::new("tab-new", IconName::Plus, NEW_TAB).on_click(cx.listener(
                    |shell, _, window, cx| {
                        shell.new_tab(&NewTab, window, cx);
                    },
                )),
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
