//! The one quit path: `⌘Q`, the palette's Quit and the close button write every note and the session first (quit spec, Q1–Q3).

use std::mem;
use std::time::Duration;

use futures::FutureExt as _;
use gpui::{Context, Task, Window};

use super::AppShell;
use crate::Quit;
use crate::vault::NoteStore;

/// A save that hangs past this lets the app quit anyway; the write keeps running until exit.
pub(super) const QUIT_WAIT: Duration = Duration::from_secs(3);

impl AppShell {
    pub(super) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(quitting) = self.begin_quit(window, cx) {
            quitting.detach();
        }
    }

    /// Resolves once the app is told to quit; `None` while a quit is already under way.
    pub(super) fn begin_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<()>> {
        if mem::replace(&mut self.quitting, true) {
            return None;
        }
        let session = self.write_session(cx);
        let flush = self.notes.update(cx, NoteStore::flush);
        let saved = futures::future::join(
            async move {
                if let Some(session) = session {
                    session.await;
                }
            },
            flush,
        );
        Some(cx.spawn_in(window, async move |_, cx| {
            futures::select_biased! {
                _ = saved.fuse() => {}
                () = cx.background_executor().timer(QUIT_WAIT).fuse() => {}
            }
            let _ = cx.update(|_, cx| cx.quit());
        }))
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
