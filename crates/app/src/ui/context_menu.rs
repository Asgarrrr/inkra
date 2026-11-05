use gpui::{
    Action, App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window, px,
};

use super::{Label, ListItem, v_flex};
use crate::theme;

const MENU_MIN_WIDTH: Pixels = px(200.);

/// A right-click's list of actions; the shell opens it in its modal layer.
pub(crate) struct ContextMenu {
    entries: Vec<(SharedString, Box<dyn Action>)>,
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
            focus: cx.focus_handle(),
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
            .track_focus(&self.focus)
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
                        ListItem::new(("context-menu-item", ix))
                            .child(Label::new(label.clone()))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(MenuClosed(Some(action.boxed_clone())));
                            }))
                    }),
            )
    }
}
