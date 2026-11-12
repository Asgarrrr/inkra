//! The one quit path: `⌘Q`, the palette's Quit and the close button write every note and the session first (quit spec, Q1–Q5).

use gpui::{Context, Window};

use super::AppShell;
use super::leave::Leave;
use crate::Quit;

impl AppShell {
    pub(super) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.start_leaving(Leave::Quit) {
            return;
        }
        let settled = self.settle(Leave::Quit, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if settled.await {
                let _ = cx.update(|_, cx| cx.quit());
            } else {
                let _ = this.update(cx, |shell, _| shell.stop_leaving(Leave::Quit));
            }
        })
        .detach();
    }

    /// Answers false while the shell lives: the quit closes the window once the writes land.
    pub(super) fn quit_on_close(window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            shell
                .update(cx, |shell, cx| shell.quit(&Quit, window, cx))
                .is_err()
        });
    }
}
