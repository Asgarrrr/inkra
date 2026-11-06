//! The tabs, in the title row over every column (tabs spec, A5; shell chrome spec, S3).

use std::cmp::Ordering;

use gpui::{
    App, AppContext, ClickEvent, Context, Div, ElementId, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render, Role, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, prelude::FluentBuilder,
};
use inkra_layout::{GroupId, View};
use inkra_tabs::TabId;
use inkra_vault::EntryId;

use super::dock_drag::DraggedView;
use super::docks::title;
use super::group::selector;
use super::parked::Parked;
use super::split::{SplitDown, SplitRight};
use super::{AppShell, CloseTab, NewTab};
use crate::one_line::one_line;
use crate::theme;
use crate::ui::{
    Color, ContextMenu, IconButton, IconName, Indicator, Label, MenuEntry, Tab, UiText, h_flex,
};
use crate::vault::NoteStore;

const NEW_TAB: &str = "New tab";

/// The tab under the pointer; also its preview. `index` is where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedTab {
    pub(super) id: TabId,
    index: usize,
    title: SharedString,
    pub(super) group: GroupId,
}

/// One tab as its bar draws it; `view` is the view it holds, if any.
struct TabEntry {
    id: TabId,
    title: SharedString,
    is_active: bool,
    indicator: Option<Indicator>,
    view: Option<View>,
}

/// The centre group a strip draws: `k` is its place in the row, for the selectors.
#[derive(Clone, Copy)]
struct Strip {
    k: usize,
    group: GroupId,
    active: bool,
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

/// A strip's slot for icon buttons, outside the bar's scroll, on the tabs' content centreline.
pub(super) fn strip_slot() -> Div {
    h_flex()
        .flex_none()
        .self_end()
        .h(theme::TAB_HEIGHT)
        .pt(theme::RULE_WIDTH)
        .pb(theme::TAB_HOVER_INSET)
}

pub(super) fn tab_title(note: Option<EntryId>, notes: &NoteStore, cx: &App) -> SharedString {
    note.and_then(|id| notes.title(id, cx))
        .map_or(SharedString::new_static(NEW_TAB), |title| {
            one_line(&title).into_owned().into()
        })
}

impl AppShell {
    /// The note each tab shows; `None` for an empty tab.
    pub(super) fn tab_notes(&self, group: GroupId) -> impl Iterator<Item = Option<EntryId>> + '_ {
        let group = self.group(group);
        let active = group.shown.note();
        group
            .tabs
            .iter()
            .map(move |tab| tab.place().map_or(active, inkra_tabs::Place::note))
    }

    /// The view each tab holds; `None` for a note's or an empty tab.
    fn tab_views(&self, group: GroupId) -> impl Iterator<Item = Option<View>> + '_ {
        let group = self.group(group);
        group
            .tabs
            .iter()
            .map(|tab| tab.place().map_or_else(|| group.shown.view(), Parked::view))
    }

    /// Every tab's title, reading the store once.
    pub(super) fn tab_titles(&self, group: GroupId, cx: &App) -> Vec<SharedString> {
        let notes = self.notes.read(cx);
        self.tab_notes(group)
            .zip(self.tab_views(group))
            .map(|(note, view)| match view {
                Some(view) => SharedString::new_static(title(view)),
                None => tab_title(note, notes, cx),
            })
            .collect()
    }

    /// Its children are the tabs only, so `scroll_to_item` indexes tabs (`div.rs:4291-4322`).
    pub(super) fn render_tab_bar(
        &self,
        k: usize,
        group: GroupId,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let strip = Strip {
            k,
            group,
            active: group == self.layout.last_center(),
        };
        let active = self.group(group).tabs.active();
        let notes = self.notes.read(cx);
        // Reading inside the draw is what repaints the dot; `Entity::cached` would break it.
        let indicators: Vec<Option<Indicator>> = self
            .tab_notes(group)
            .map(|note| {
                note.and_then(|id| Indicator::of(notes.is_dirty(id, cx), notes.save_failed(id)))
            })
            .collect();
        let views: Vec<_> = self.tab_views(group).collect();
        let tabs: Vec<_> = self
            .group(group)
            .tabs
            .iter()
            .zip(self.tab_titles(group, cx))
            .zip(indicators)
            .zip(views)
            .enumerate()
            .map(|(ix, (((tab, title), indicator), view))| {
                let entry = TabEntry {
                    id: tab.id(),
                    title,
                    is_active: tab.id() == active,
                    indicator,
                    view,
                };
                Self::render_tab(strip, ix, entry, cx)
            })
            .collect();
        div()
            .id("tab-bar")
            .debug_selector(move || selector("tab-bar", k))
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
            .track_scroll(&self.group(group).tab_scroll)
            .ui_text()
            .children(tabs)
    }

    fn render_tab(
        strip: Strip,
        ix: usize,
        TabEntry {
            id,
            title,
            is_active,
            indicator,
            view,
        }: TabEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = theme::current(cx).colors;
        let dragged = DraggedTab {
            id,
            index: ix,
            title: title.clone(),
            group: strip.group,
        };
        let group = strip.group;
        let landing = move |style: StyleRefinement, own: Option<usize>| {
            let style = style
                .bg(colors.accent_background)
                .border_color(colors.accent);
            match own.map_or(Ordering::Less, |index| ix.cmp(&index)) {
                Ordering::Less => style.border_l(theme::DROP_EDGE),
                Ordering::Greater => style.border_r(theme::DROP_EDGE),
                Ordering::Equal => style,
            }
        };
        Tab::new(("tab", ix))
            .debug_selector(move || selector(format_args!("tab-{ix}"), strip.k))
            .active(is_active)
            .when_some(indicator, |tab, indicator| {
                tab.start(
                    div()
                        .debug_selector(move || selector(indicator.selector("tab", ix), strip.k))
                        .child(indicator),
                )
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press_tab))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::press_tab))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.open_tab_menu(group, id, event.position, window, cx);
                }),
            )
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.activate_tab(group, id, window, cx);
            }))
            // The × has no aux listener, so a middle click on it closes its tab here.
            .on_aux_click(cx.listener(move |shell, event: &ClickEvent, window, cx| {
                if event.is_middle_click() {
                    cx.stop_propagation();
                    shell.close_tab(group, id, window, cx);
                }
            }))
            .map(|tab| match view {
                Some(view) => tab.on_drag(
                    DraggedView {
                        view,
                        index: ix,
                        group,
                    },
                    |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                ),
                None => tab.on_drag(dragged, |tab, _, _, cx| cx.new(|_| tab.clone())),
            })
            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                landing(style, (dragged.group == group).then_some(dragged.index))
            })
            .on_drop(cx.listener(move |shell, dragged: &DraggedTab, window, cx| {
                shell.move_tab_to(dragged.group, dragged.id, group, ix, window, cx);
            }))
            .drag_over::<DraggedView>(move |style, dragged, _, _| {
                landing(style, (dragged.group == group).then_some(dragged.index))
            })
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedView, window, cx| {
                    shell.move_view(dragged.view, group, ix, window, cx);
                }),
            )
            .map(|tab| {
                if is_active && !strip.active {
                    tab.child(Label::new(title).color(Color::Muted))
                } else {
                    tab.child(title)
                }
            })
            .end(
                IconButton::new(("tab-close", ix), IconName::X, "Close tab")
                    .square(theme::TAB_CLOSE_SIZE)
                    .radius(theme::TAB_CLOSE_RADIUS)
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // One click, one action: the tab under the × must not also see it.
                        cx.stop_propagation();
                        shell.close_tab(group, id, window, cx);
                    })),
            )
    }

    /// After the bar, not in it: `scroll_to_item` indexes the bar's children, which are tabs only.
    pub(super) fn render_new_tab(k: usize, cx: &mut Context<Self>) -> impl IntoElement {
        strip_slot().child(
            IconButton::new(
                ElementId::Name(selector("tab-new", k).into()),
                IconName::Plus,
                NEW_TAB,
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.new_tab(&NewTab, window, cx);
            })),
        )
    }

    /// The menu's entries act on the focused group, which the press has just made `group`.
    /// `prevent_default`: the root would take focus from the menu on the press (`div.rs:2765-2780`).
    fn open_tab_menu(
        &mut self,
        group: GroupId,
        id: TabId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.prevent_default();
        self.activate_group(group, window, cx);
        self.activate_tab(group, id, window, cx);
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
        let entries = vec![
            MenuEntry::new("Split Right", Box::new(SplitRight)),
            MenuEntry::new("Split Down", Box::new(SplitDown)),
            MenuEntry::new("Close", Box::new(CloseTab)),
        ];
        let menu = cx.new(|cx| ContextMenu::new(entries, cx));
        self.open_menu(menu, at, window, cx);
    }

    /// Focuses the active tab's content: gpui's root focus-on-press (div.rs:2765-2780) is prevented.
    fn press_tab(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.prevent_default();
        if let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }
}
