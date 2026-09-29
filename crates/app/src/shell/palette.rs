//! `⌘⇧P` on the shell: open the palette for the focused handle, run the chosen command on it.

use gpui::{Context, Window};

use super::{AppShell, remember};
use crate::command_palette::{self, CommandChosen, CommandPaletteDelegate};

impl AppShell {
    /// Closes any other picker first, so the list, its keys and the dispatch answer for one handle (palette spec D1).
    pub(super) fn command_palette(
        &mut self,
        _: &command_palette::Toggle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_open = self.modal_is::<CommandPaletteDelegate>();
        self.dismiss_modal(window, cx);
        if was_open {
            return;
        }
        // The list is built for this handle, so the command runs on it even if a note lands meanwhile (palette spec D4).
        let target = window
            .focused(cx)
            .unwrap_or_else(|| self.focus_handle.clone());
        let commands = command_palette::commands(window, cx, &target, &self.recent_commands);
        let Some(picker) =
            self.toggle_picker(window, cx, |_| CommandPaletteDelegate::new(commands))
        else {
            return;
        };
        // With a window, unlike `toggle_picker_with`: focusing, dismissing and dispatching all need one.
        let chosen = cx.subscribe_in(
            &picker,
            window,
            move |shell, _, event: &CommandChosen, window, cx| {
                remember(&mut shell.recent_commands, event.0.name());
                // As Zed: focus first, so `dismiss_modal` finds the picker unfocused and leaves focus alone.
                let focus = if shell.draws(&target, window) {
                    &target
                } else {
                    &shell.focus_handle
                };
                window.focus(focus, cx);
                shell.dismiss_modal(window, cx);
                window.dispatch_action(event.0.boxed_clone(), cx);
            },
        );
        if let Some(modal) = &mut self.modal {
            modal.hold(chosen);
        }
    }
}
