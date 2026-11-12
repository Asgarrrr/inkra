//! The one line reporting a failure: a save, a rename or an open that failed, or a conflict copy kept (quit-and-failures spec, Q9).

use std::path::Path;

use gpui::{
    App, Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
};
use inkra_vault::{EntryId, VaultError};

use super::AppShell;
use crate::sidebar::title_of;
use crate::theme;
use crate::ui::{Color, IconButton, IconName};
use crate::vault::{ConflictPreserved, NoteSaved, NoteStore, OpenFailed, RenameFailed, SaveFailed};

pub(super) struct Notice {
    pub(super) text: SharedString,
    /// The note whose next landed save makes the notice obsolete.
    pub(super) cleared_by_save: Option<EntryId>,
}

impl Notice {
    /// The platform refused to watch the vault; no save clears it.
    pub(super) fn unwatched(error: &VaultError) -> Self {
        Self {
            text: format!(
                "Inkra can't follow changes made outside the app: {}",
                reason(error)
            )
            .into(),
            cleared_by_save: None,
        }
    }
}

impl AppShell {
    /// All detached, as the shell's other subscriptions: the store reports, the shell words it.
    pub(super) fn watch_failures(notes: &Entity<NoteStore>, cx: &mut Context<Self>) {
        cx.subscribe(notes, |shell, _notes, event: &SaveFailed, cx| {
            let title = shell.title(event.id, cx);
            let text = match &event.error {
                Some(error) => format!("Couldn't save “{title}”: {}", reason(error)),
                None => format!("Couldn't save “{title}”: its file is no longer in the vault."),
            };
            shell.show_notice(text, Some(event.id), cx);
        })
        .detach();
        cx.subscribe(notes, |shell, _notes, event: &RenameFailed, cx| {
            let title = shell.title(event.id, cx);
            let text = format!("Couldn't rename “{title}”: {}", reason(&event.error));
            shell.show_notice(text, None, cx);
        })
        .detach();
        cx.subscribe(notes, |shell, _notes, event: &OpenFailed, cx| {
            let title = shell.title(event.id, cx);
            let text = format!("Couldn't open “{title}”: {}", reason(&event.error));
            shell.show_notice(text, None, cx);
        })
        .detach();
        cx.subscribe(notes, |shell, _notes, event: &ConflictPreserved, cx| {
            let title = shell.title(event.id, cx);
            let copy = event.path.file_name().unwrap_or_default().to_string_lossy();
            let text = format!(
                "“{title}” changed on disk while you edited it. The disk version is kept as “{copy}”."
            );
            shell.show_notice(text, None, cx);
        })
        .detach();
        // `write` emits `NoteSaved` before `ConflictPreserved`, so a conflict notice survives its own write.
        cx.subscribe(notes, |shell, _notes, event: &NoteSaved, cx| {
            if shell
                .notice
                .as_ref()
                .is_some_and(|notice| notice.cleared_by_save == Some(event.id))
            {
                shell.notice = None;
                cx.notify();
            }
        })
        .detach();
    }

    pub(crate) fn show_notice(
        &mut self,
        text: String,
        cleared_by_save: Option<EntryId>,
        cx: &mut Context<Self>,
    ) {
        self.notice = Some(Notice {
            text: text.into(),
            cleared_by_save,
        });
        cx.notify();
    }

    /// Tells the first failed write of the shell this one replaced, by file name: that note left with it.
    pub(super) fn left_unsaved(&mut self, errors: &[VaultError], cx: &mut Context<Self>) {
        for error in errors {
            log::error!("save left behind: {error}");
        }
        if let Some(first) = errors.first() {
            let stem = failed_path(first).file_stem().unwrap_or_default();
            let text = format!(
                "Couldn't save “{}”: {}",
                stem.to_string_lossy(),
                reason(first)
            );
            self.show_notice(text, None, cx);
        }
    }

    fn title(&self, id: EntryId, cx: &App) -> String {
        self.notes
            .read(cx)
            .title(id, cx)
            .unwrap_or_else(|| "Untitled".into())
    }

    pub(super) fn render_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let notice = self.notice.as_ref()?;
        let colors = theme::current(cx).colors;
        Some(
            div()
                .debug_selector(|| "notice".into())
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_4()
                .bg(colors.error_background)
                .text_size(theme::UI_TEXT_SIZE)
                .line_height(theme::UI_LINE_HEIGHT)
                .text_color(Color::Error.rgba(theme::current(cx)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(notice.text.clone()),
                )
                .child(
                    IconButton::new("notice-dismiss", IconName::X, "Dismiss")
                        .debug_selector(|| "notice-dismiss".into())
                        .on_click(cx.listener(|shell, _, _, cx| {
                            shell.notice = None;
                            cx.notify();
                        })),
                ),
        )
    }
}

/// The folder a vault open could not reach, named as the tree's header names it.
pub(crate) fn couldnt_open(root: &Path, error: &VaultError) -> String {
    format!("Couldn't open “{}”: {}", title_of(root), reason(error))
}

/// The note a failed write was for, under an `Unwritten`'s preserving save.
fn failed_path(error: &VaultError) -> &Path {
    match error {
        VaultError::Io { path, .. } | VaultError::NotUtf8 { path } => path,
        VaultError::Unwritten { source, .. } => failed_path(source),
    }
}

/// The OS's words, without the path: the notice names the note by its title.
fn reason(error: &VaultError) -> String {
    match error {
        VaultError::Io { source, .. } => source.to_string(),
        VaultError::NotUtf8 { .. } => "not valid UTF-8".into(),
        VaultError::Unwritten { source, .. } => reason(source),
    }
}
