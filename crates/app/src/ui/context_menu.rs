use gpui::{
    Action, App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    prelude::FluentBuilder, px,
};

use super::{Label, ListItem, v_flex};
use crate::field::{Cancel, Confirm};
use crate::{picker, theme};

const MENU_MIN_WIDTH: Pixels = px(200.);

/// A right-click's list of actions; the shell opens it in its modal layer.
pub(crate) struct ContextMenu {
    entries: Vec<(SharedString, Box<dyn Action>)>,
    selected: Option<usize>,
    focus: FocusHandle,
}

/// The chosen action, or `None` when the menu closes on nothing.
pub(crate) struct MenuClosed(pub(crate) Option<Box<dyn Action>>);

impl EventEmitter<MenuClosed> for ContextMenu {}

impl ContextMenu {
    pub(crate) fn new(
        entries: Vec<(SharedString, Box<dyn Action>)>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            cx.emit(MenuClosed(Some(self.entries[ix].1.boxed_clone())));
        }
    }

    fn cancel(_: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(MenuClosed(None));
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
            .on_action(cx.listener(|_, action, window, cx| Self::cancel(action, window, cx)))
            .debug_selector(|| "context-menu".into())
            .min_w(MENU_MIN_WIDTH)
            .py_1()
            .bg(colors.elevated_background)
            .border_1()
            .border_color(colors.border)
            .rounded_md()
            .shadow_lg()
            .children(
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(ix, (label, action))| {
                        let action = action.boxed_clone();
                        let selected = self.selected == Some(ix);
                        ListItem::new(("context-menu-item", ix))
                            .selected(selected)
                            // A test window draws no background, so the selector carries the state.
                            .when(selected, |row| {
                                row.debug_selector(move || {
                                    format!("context-menu-item-{ix}-selected")
                                })
                            })
                            .child(Label::new(label.clone()))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(MenuClosed(Some(action.boxed_clone())));
                            }))
                    }),
            )
    }
}
