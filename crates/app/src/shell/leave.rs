//! Leaving a shell: quit and open share one settle (open-a-vault spec, D3).

use std::time::Duration;

use futures::FutureExt as _;
use gpui::{AsyncWindowContext, Context, PromptLevel, Task, Window};

use super::AppShell;
use crate::vault::NoteStore;

/// A leave waits this long for its writes and reads; past it, a quit proceeds and an open stops.
pub(super) const QUIT_WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Leave {
    Quit,
}

impl Leave {
    /// The unsaved alert's first button.
    fn anyway(self) -> &'static str {
        match self {
            Self::Quit => "Quit Anyway",
        }
    }
}

impl AppShell {
    /// False while another leave is under way: a second one does nothing.
    pub(super) fn start_leaving(&mut self, leave: Leave) -> bool {
        match (self.leaving, leave) {
            (None, _) => {
                self.leaving = Some(leave);
                true
            }
            (Some(_), _) => false,
        }
    }

    /// Ends `leave`; false once another leave has taken over.
    pub(super) fn stop_leaving(&mut self, leave: Leave) -> bool {
        self.leaving.take_if(|l| *l == leave).is_some()
    }

    /// Writes the session and every note, then asks about what is still unsaved; true to go on.
    /// It stops once `leave` is no longer the leave under way, and never writes `leaving`.
    pub(super) fn settle(
        &mut self,
        leave: Leave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
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
            let still = |cx: &mut AsyncWindowContext| {
                this.read_with(cx, |shell, _| shell.leaving == Some(leave))
                    .unwrap_or(false)
            };
            if !still(cx) {
                return false;
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
                return true;
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
                    &[leave.anyway(), "Cancel"],
                    cx,
                )
            }) else {
                return false;
            };
            answer.await == Ok(0) && still(cx)
        })
    }
}
