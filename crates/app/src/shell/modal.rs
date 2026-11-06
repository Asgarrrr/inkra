//! One modal at a time over the whole window, a picker or a menu, after Zed's `ModalLayer`.

use gpui::{
    AnyView, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, Styled,
    Subscription, Window, anchored, div, px, relative,
};

use super::AppShell;
use crate::picker::{Dismissed, Picker, PickerDelegate};
use crate::theme;
use crate::ui::{ContextMenu, MenuClosed};

const MODAL_WIDTH: Pixels = px(600.);

pub(super) struct Modal {
    view: AnyView,
    /// Focused before the first modal opened; a replacing modal keeps it.
    restore: Option<FocusHandle>,
    /// The picker's field, or the menu.
    focus: FocusHandle,
    /// `None` centres a picker; a menu is anchored here.
    at: Option<Point<Pixels>>,
    subscriptions: Vec<Subscription>,
}

impl Modal {
    pub(super) fn hold(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    pub(super) fn restore_to(&mut self, focus: FocusHandle) {
        self.restore = Some(focus);
    }

    pub(super) fn restores(&self, focus: &FocusHandle) -> bool {
        self.restore.as_ref() == Some(focus)
    }
}

impl AppShell {
    /// Opens a picker over the window, or closes it if one of the same kind is showing.
    pub(super) fn toggle_picker<D: PickerDelegate>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        delegate: impl FnOnce(&mut Context<Picker<D>>) -> D,
    ) -> Option<Entity<Picker<D>>> {
        if self.modal_is::<D>() {
            self.dismiss_modal(window, cx);
            return None;
        }
        let restore = self.restore_focus(window, cx);
        let picker = cx.new(|cx| {
            let delegate = delegate(cx);
            Picker::new(delegate, window, cx)
        });
        let focus = picker.focus_handle(cx);
        let dismissed = cx.subscribe_in(&picker, window, |shell, _, _: &Dismissed, window, cx| {
            shell.dismiss_modal(window, cx);
        });
        self.modal = Some(Modal {
            view: picker.clone().into(),
            restore,
            focus: focus.clone(),
            at: None,
            subscriptions: vec![dismissed],
        });
        window.focus(&focus, cx);
        cx.notify();
        Some(picker)
    }

    /// Where focus returns at the dismissal: a replaced modal's return, or the focus now.
    fn restore_focus(&self, window: &Window, cx: &App) -> Option<FocusHandle> {
        match &self.modal {
            Some(modal) => modal.restore.clone(),
            None => window.focused(cx),
        }
    }

    /// Opens `menu` at `at`. A choice dispatches once focus is back, so it reaches the view the menu left.
    pub(super) fn open_menu(
        &mut self,
        menu: Entity<ContextMenu>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let restore = self.restore_focus(window, cx);
        let focus = menu.focus_handle(cx);
        let closed = cx.subscribe_in(&menu, window, |shell, _, event: &MenuClosed, window, cx| {
            let action = event.0.as_ref().map(|action| action.boxed_clone());
            shell.dismiss_modal(window, cx);
            if let Some(action) = action {
                window.dispatch_action(action, cx);
            }
        });
        self.modal = Some(Modal {
            view: menu.into(),
            restore,
            focus: focus.clone(),
            at: Some(at),
            subscriptions: vec![closed],
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// As `toggle_picker`, and maps each `E` the picker emits to the shell for as long as it is open.
    pub(super) fn toggle_picker_with<D, E: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        delegate: impl FnOnce(&mut Context<Picker<D>>) -> D,
        on_pick: impl Fn(&mut Self, &E, &mut Context<Self>) + 'static,
    ) where
        D: PickerDelegate,
        Picker<D>: EventEmitter<E>,
    {
        let Some(picker) = self.toggle_picker(window, cx, delegate) else {
            return;
        };
        let subscription = cx.subscribe(&picker, move |shell, _, event: &E, cx| {
            on_pick(shell, event, cx);
        });
        if let Some(modal) = &mut self.modal {
            modal.hold(subscription);
        }
    }

    pub(super) fn modal_picker<D: PickerDelegate>(&self) -> Option<Entity<Picker<D>>> {
        self.modal.as_ref()?.view.clone().downcast().ok()
    }

    pub(super) fn modal_menu(&self) -> Option<Entity<ContextMenu>> {
        self.modal.as_ref()?.view.clone().downcast().ok()
    }

    pub(super) fn modal_is<D: PickerDelegate>(&self) -> bool {
        self.modal_picker::<D>().is_some()
    }

    /// Focus goes back only if the picker still holds it: a click elsewhere already moved it.
    pub(super) fn dismiss_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        if modal.focus.is_focused(window) {
            let restore = modal
                .restore
                .filter(|restore| self.draws(restore, window))
                .unwrap_or_else(|| self.focus_handle(cx));
            window.focus(&restore, cx);
        }
        cx.notify();
    }

    /// Whether the last frame drew `handle`: focus on a dropped view's handle reaches no action.
    pub(super) fn draws(&self, handle: &FocusHandle, window: &Window) -> bool {
        self.focus_handle.contains(handle, window)
    }

    /// Both layers occlude: gpui's hit test stops only at an occluding hitbox (`gpui/src/window.rs:1106-1123`).
    /// A menu also closes on a right press outside, as macOS's do.
    pub(super) fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let modal = self.modal.as_ref()?;
        let colors = theme::current(cx).colors;
        let overlay = div().absolute().inset_0().occlude().on_mouse_down(
            MouseButton::Left,
            cx.listener(|shell, _, window, cx| shell.dismiss_modal(window, cx)),
        );
        Some(match modal.at {
            Some(at) => overlay
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|shell, _, window, cx| shell.dismiss_modal(window, cx)),
                )
                .child(
                    anchored().position(at).snap_to_window().child(
                        div()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(modal.view.clone()),
                    ),
                ),
            None => overlay.child(
                div()
                    .absolute()
                    .top(relative(0.15))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .occlude()
                            .w(MODAL_WIDTH)
                            .bg(colors.elevated_background)
                            .border_1()
                            .border_color(colors.border)
                            .rounded(theme::ELEVATED_RADIUS)
                            .shadow_lg()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(modal.view.clone()),
                    ),
            ),
        })
    }
}
