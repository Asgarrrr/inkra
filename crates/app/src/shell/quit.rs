//! The one quit path: `⌘Q`, the palette's Quit and the close button write every note and the session first (quit spec, Q1–Q5).

use std::mem;
use std::time::Duration;

use futures::FutureExt as _;
use gpui::{Context, PromptLevel, Window};

use super::AppShell;
use crate::Quit;
use crate::vault::NoteStore;

/// A save that hangs past this lets the app quit anyway; the write keeps running until exit.
pub(super) const QUIT_WAIT: Duration = Duration::from_secs(3);

impl AppShell {
    pub(super) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if mem::replace(&mut self.quitting, true) {
            return;
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
        let notes = self.notes.clone();
        cx.spawn_in(window, async move |this, cx| {
            futures::select_biased! {
                _ = saved.fuse() => {}
                () = cx.background_executor().timer(QUIT_WAIT).fuse() => {}
            }
            let titles = notes.read_with(cx, |notes, cx| {
                let mut titles: Vec<String> = notes
                    .unsaved(cx)
                    .into_iter()
                    .map(|id| notes.title(id, cx).unwrap_or_else(|| "Untitled".into()))
                    .collect();
                titles.sort();
                titles
            });
            if titles.is_empty() {
                let _ = cx.update(|_, cx| cx.quit());
                return;
            }
            let detail = format!(
                "Not on disk: {}.",
                titles
                    .iter()
                    .map(|title| format!("“{title}”"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let Ok(answer) = cx.update(|window, cx| {
                window.prompt(
                    PromptLevel::Warning,
                    "Some changes are not saved.",
                    Some(&detail),
                    &["Quit Anyway", "Cancel"],
                    cx,
                )
            }) else {
                return;
            };
            if answer.await == Ok(0) {
                let _ = cx.update(|_, cx| cx.quit());
            } else {
                let _ = this.update(cx, |shell, _| shell.quitting = false);
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
