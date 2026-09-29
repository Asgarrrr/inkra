//! One picker at a time over the whole window, after Zed's `ModalLayer`.

use gpui::{
    AnyView, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Styled, Subscription, Window, div, px,
    relative,
};

use super::AppShell;
use crate::picker::{Dismissed, Picker, PickerDelegate};
use crate::theme;

const MODAL_WIDTH: Pixels = px(600.);

pub(super) struct Modal {
    view: AnyView,
    /// Focused before the first picker opened; a replacing picker keeps it.
    restore: Option<FocusHandle>,
    /// The picker's field.
    focus: FocusHandle,
    subscriptions: Vec<Subscription>,
}

impl Modal {
    pub(super) fn hold(&mut self, subscription: Subscription) {
        self.subscriptions.push(subscription);
    }

    pub(super) fn restore_to(&mut self, focus: FocusHandle) {
        self.restore = Some(focus);
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
        let mut restore = window.focused(cx);
        if let Some(modal) = &self.modal {
            restore.clone_from(&modal.restore);
        }
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
            subscriptions: vec![dismissed],
        });
        window.focus(&focus, cx);
        cx.notify();
        Some(picker)
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
    pub(super) fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let modal = self.modal.as_ref()?;
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| shell.dismiss_modal(window, cx)),
                )
                .child(
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
                                .bg(theme::modal_background())
                                .border_1()
                                .border_color(theme::modal_border())
                                .rounded_md()
                                .shadow_lg()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(modal.view.clone()),
                        ),
                ),
        )
    }
}
