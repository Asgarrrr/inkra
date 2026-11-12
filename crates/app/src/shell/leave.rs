//! Leaving a shell: quit and open share one settle (open-a-vault spec, D3).

use std::time::Duration;

use futures::FutureExt as _;
use futures::future::Shared;
use gpui::{App, AsyncWindowContext, Context, PromptLevel, Task, Window};
use inkra_vault::VaultError;

use super::AppShell;
use crate::vault::NoteStore;

/// A leave's settle waits this long for its writes, then asks; an open's read stops past it.
pub(super) const QUIT_WAIT: Duration = Duration::from_secs(3);

/// The writes every replaced shell left behind and that may still be pending: the next settle joins them.
#[derive(Clone)]
pub(crate) struct Left(Shared<Task<()>>);

impl Default for Left {
    fn default() -> Self {
        Self(Task::ready(()).shared())
    }
}

impl Left {
    /// These writes, then `writes`; detached as well as held, so a dropped `Left` cancels no write.
    pub(super) fn then(
        self,
        writes: impl Future<Output = Vec<VaultError>> + 'static,
        window: &Window,
        cx: &mut App,
    ) -> Self {
        let task = window
            .spawn(cx, async move |cx| {
                let errors = writes.await;
                let _ = cx.update(|window, cx| {
                    if let Some(Some(shell)) = window.root::<AppShell>() {
                        shell.update(cx, |shell, cx| shell.left_unsaved(&errors, cx));
                    }
                });
                self.0.await;
            })
            .shared();
        cx.foreground_executor().spawn(task.clone()).detach();
        Self(task)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Leave {
    Quit,
    Open,
}

impl Leave {
    /// The unsaved alert's first button.
    fn anyway(self) -> &'static str {
        match self {
            Self::Quit => "Quit Anyway",
            Self::Open => "Open Anyway",
        }
    }
}

impl AppShell {
    /// False while another leave is under way: a second one does nothing, except a quit over an open.
    fn start_leaving(&mut self, leave: Leave) -> bool {
        match (self.leaving, leave) {
            (None, _) | (Some(Leave::Open), Leave::Quit) => {
                self.leaving = Some(leave);
                true
            }
            (Some(_), _) => false,
        }
    }

    pub(super) fn start_quit(&mut self) -> bool {
        self.start_leaving(Leave::Quit)
    }

    /// Starts an open; false while a leave is under way or an earlier open's task is still alive.
    pub(super) fn start_open(&mut self) -> bool {
        if self.opening_vault || !self.start_leaving(Leave::Open) {
            return false;
        }
        self.opening_vault = true;
        true
    }

    /// Ends the open's task; false once a quit has taken its leave.
    pub(super) fn stop_open(&mut self) -> bool {
        self.opening_vault = false;
        self.stop_leaving(Leave::Open)
    }

    /// Ends `leave`; false once another leave has taken over.
    pub(super) fn stop_leaving(&mut self, leave: Leave) -> bool {
        self.leaving.take_if(|l| *l == leave).is_some()
    }

    /// Writes the session and every note, then asks about the unsaved; false once `leave` is not under way.
    pub(super) fn settle(
        &mut self,
        leave: Leave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if self.leaving != Some(leave) {
            return Task::ready(false);
        }
        let session = self.write_session(cx);
        let flush = self.notes.update(cx, NoteStore::flush);
        let saved = futures::future::join3(
            async move {
                if let Some(session) = session {
                    session.await;
                }
            },
            flush,
            self.left.0.clone(),
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
