use gpui::{
    Action, App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    prelude::FluentBuilder, px,
};

use super::{Icon, IconName, Indicator, Label, ListItem, h_flex, v_flex};
use crate::field::{Cancel, Confirm};
use crate::{picker, theme};

const MENU_MIN_WIDTH: Pixels = px(200.);

/// A row of a menu: a label, an optional leading icon, and the action it runs.
pub(crate) struct MenuEntry {
    label: SharedString,
    icon: Option<IconName>,
    /// `None` runs nothing and closes the menu.
    action: Option<Box<dyn Action>>,
    /// Drawn with the current dot.
    current: bool,
}

impl MenuEntry {
    pub(crate) fn new(label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            action: Some(action),
            current: false,
        }
    }

    pub(crate) fn current(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            action: None,
            current: true,
        }
    }

    pub(crate) fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    fn action(&self) -> Option<Box<dyn Action>> {
        self.action.as_ref().map(|action| action.boxed_clone())
    }
}

/// A right-click's list of actions; the shell opens it in its modal layer.
pub(crate) struct ContextMenu {
    entries: Vec<MenuEntry>,
    selected: Option<usize>,
    focus: FocusHandle,
}

/// The chosen action, or `None` when the menu closes on nothing.
pub(crate) struct MenuClosed(pub(crate) Option<Box<dyn Action>>);

impl EventEmitter<MenuClosed> for ContextMenu {}

impl ContextMenu {
    pub(crate) fn new(entries: Vec<MenuEntry>, cx: &mut Context<Self>) -> Self {
        assert!(!entries.is_empty(), "a context menu needs an entry");
        Self {
            entries,
            selected: None,
            focus: cx.focus_handle(),
        }
    }

    fn select_next(&mut self, _: &picker::SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let len = self.entries.len();
        self.selected = Some(self.selected.map_or(0, |ix| (ix + 1) % len));
        cx.notify();
    }

    fn select_prev(&mut self, _: &picker::SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        let len = self.entries.len();
        self.selected = Some(match self.selected {
            Some(ix) if ix > 0 => ix - 1,
            _ => len - 1,
        });
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.selected {
            cx.emit(MenuClosed(self.entries[ix].action()));
        }
    }
}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ContextMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::current(cx).colors;
        v_flex()
            .key_context("ContextMenu")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(MenuClosed(None))))
            .debug_selector(|| "context-menu".into())
            .min_w(MENU_MIN_WIDTH)
            .p(theme::MENU_INSET)
            .bg(colors.elevated_background)
            .border_1()
            .border_color(colors.border)
            .rounded(theme::ELEVATED_RADIUS)
            .shadow_lg()
            .children(self.entries.iter().enumerate().map(|(ix, entry)| {
                let selected = self.selected == Some(ix);
                let action = entry.action();
                ListItem::new(("context-menu-item", ix))
                    .selected(selected)
                    .when_some(entry.icon, |row, icon| row.start(Icon::new(icon)))
                    .when(entry.current, |row| {
                        let current = Indicator::Current;
                        row.start(
                            h_flex()
                                .size(theme::ICON_SIZE)
                                .justify_center()
                                .child(current),
                        )
                        .debug_selector(move || current.selector("context-menu-item", ix))
                    })
                    // A test window draws no background, so the selector carries the state.
                    .when(selected, |row| {
                        row.debug_selector(move || format!("context-menu-item-{ix}-selected"))
                    })
                    .child(Label::new(entry.label.clone()))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(MenuClosed(
                            action.as_ref().map(|action| action.boxed_clone()),
                        ));
                    }))
            }))
    }
}
