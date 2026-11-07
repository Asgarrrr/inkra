//! What a tab does with a file by its kind: the one place that matches on `FileKind`, but for the
//! tree's icon (file-kinds spec, K3).

use std::collections::HashSet;

use gpui::{
    AnyElement, App, Context, Entity, ImageSource, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Styled, StyledImage, Task, Window, div, img,
};
use inkra_layout::GroupId;
use inkra_text::Buffer;
use inkra_vault::EntryId;

use super::parked::Parked;
use super::tabs::OnFile;
use super::{AppShell, Shown};
use crate::editor::{EditorView, ParkedView, ViewState};
use crate::file_kind::FileKind;
use crate::images::Slot;
use crate::sidebar::Cursor;
use crate::ui::{Color, Label};

/// An open begun: a note's load, or an image, which `show_image` loads as it shows.
pub(super) enum Opening {
    Note(Task<Option<Entity<Buffer>>>),
    Image,
}

impl Opening {
    /// `None` when the note's load failed.
    pub(super) async fn landed(self) -> Option<Landing> {
        match self {
            Self::Note(load) => load.await.map(Landing::Note),
            Self::Image => Some(Landing::Image),
        }
    }
}

pub(super) enum Landing {
    Note(Entity<Buffer>),
    Image,
}

impl AppShell {
    /// A note with no entry, gone or detached, is still a note while its buffer lives.
    pub(super) fn file_kind(&self, id: EntryId, cx: &App) -> Option<FileKind> {
        let notes = self.notes.read(cx);
        match notes.vault().and_then(|vault| vault.read(cx).entry(id)) {
            Some(entry) if entry.kind.is_file() => FileKind::of(entry.path.as_path()),
            Some(_) => None,
            None => notes.buffer_for(id).map(|_| FileKind::Note),
        }
    }

    pub(super) fn opening(
        &mut self,
        id: EntryId,
        kind: FileKind,
        cx: &mut Context<Self>,
    ) -> Opening {
        match kind {
            FileKind::Note => {
                Opening::Note(self.notes.update(cx, |notes, cx| notes.open_note(id, cx)))
            }
            FileKind::Image => Opening::Image,
        }
    }

    /// Shows a landed open in `group`; the editor is returned only for a note.
    pub(super) fn show_landed(
        &mut self,
        group: GroupId,
        id: EntryId,
        landing: Landing,
        view: Option<ViewState>,
        since: Cursor,
        cx: &mut Context<Self>,
    ) -> Option<Entity<EditorView>> {
        match landing {
            Landing::Note(buffer) => Some(self.bind(group, id, buffer, view, since, cx)),
            Landing::Image => {
                self.show_image(group, id, since, cx);
                None
            }
        }
    }

    /// An inactive tab on a landed open; an image keeps no view.
    pub(super) fn park_landed(
        id: EntryId,
        landing: Landing,
        view: Option<ViewState>,
        cx: &mut App,
    ) -> Parked {
        let view = match landing {
            Landing::Note(buffer) => view.map(|view| ParkedView::new(buffer, view, cx)),
            Landing::Image => None,
        };
        Parked::new(id, view)
    }

    /// The kind is read now, so a rename that changed it shows the file as it is now.
    pub(super) fn display_file(
        &mut self,
        group: GroupId,
        on: OnFile<ViewState>,
        since: Cursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = self.notes.read(cx).buffer_for(on.file).cloned();
        match (self.file_kind(on.file, cx), buffer) {
            (Some(FileKind::Note) | None, Some(buffer)) => {
                self.bind(group, on.file, buffer, on.view, since, cx);
            }
            // A held note with no buffer has its trash pending.
            (Some(FileKind::Note) | None, None) if self.notes.read(cx).holds(on.file) => {
                self.unfocus_editor(group, window, cx);
                self.set_shown(group, Shown::Trashing(on.file), since, cx);
            }
            // An image, a file renamed out of a note, or one no kind shows: it decodes or shows "can't show".
            (Some(FileKind::Image), _) | (Some(FileKind::Note) | None, None) => {
                self.unfocus_editor(group, window, cx);
                self.show_image(group, on.file, since, cx);
            }
        }
    }

    /// Shows `id` at once; its load lands later. Needs no window.
    pub(super) fn show_image(
        &mut self,
        group: GroupId,
        id: EntryId,
        since: Cursor,
        cx: &mut Context<Self>,
    ) {
        if let Some(panes) = &self.panes {
            let path = panes.vault.read(cx).absolute(id);
            panes
                .images
                .update(cx, |images, cx| images.want(id, path, cx));
        }
        self.set_shown(group, Shown::Image(id), since, cx);
    }

    /// The bare group area while the image loads.
    pub(super) fn render_image(&self, id: EntryId, cx: &App) -> AnyElement {
        let slot = self
            .panes
            .as_ref()
            .and_then(|panes| panes.images.read(cx).slot(id));
        match slot {
            Some(Slot::Ready(image)) => img(ImageSource::Render(image.clone()))
                .object_fit(ObjectFit::ScaleDown)
                .size_full()
                .debug_selector(|| "image-view".into())
                .into_any_element(),
            Some(Slot::Failed) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .debug_selector(|| "image-failed".into())
                        .child(Label::new("Inkra can't show this file.").color(Color::Muted)),
                )
                .into_any_element(),
            Some(Slot::Loading { .. }) | None => div().size_full().into_any_element(),
        }
    }

    /// Deferred, so a tab moved between groups in one update keeps its image.
    pub(super) fn release_images_later(cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, Self::release_images);
        });
    }

    fn release_images(&mut self, cx: &mut Context<Self>) {
        let Some(panes) = &self.panes else {
            return;
        };
        let keep: HashSet<EntryId> = self
            .layout
            .center_groups()
            .into_iter()
            .filter_map(|group| match self.group(group).shown {
                Shown::Image(id) => Some(id),
                Shown::Empty | Shown::Bound(_) | Shown::Trashing(_) | Shown::View(_) => None,
            })
            .collect();
        panes
            .images
            .update(cx, |images, cx| images.retain(&keep, cx));
    }
}
