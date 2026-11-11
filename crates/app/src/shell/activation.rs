//! Opening a note into the editor, and following a link from it.

use std::ops::Range;

use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable, Window};
use inkra_index::{Resolution, Target};
use inkra_layout::{GroupId, SplitDirection, View};
use inkra_markdown::Link;
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::EntryId;

use super::group::Group;
use super::surface::Landing;
use super::tabs::{Place, capture};
use super::{AppShell, Bound, Shown, remember};
use crate::editor::{EditorView, LinkFollowed, ViewState};
use crate::file_kind::FileKind;
use crate::open::Open;
use crate::sidebar::Cursor;
use crate::vault::NoteStore;

/// Where an open began: the group it lands in, and the tree's cursor the highlight follows from.
#[derive(Clone, Copy)]
pub(super) struct Origin {
    pub(super) group: GroupId,
    pub(super) since: Cursor,
}

impl AppShell {
    /// Commits into the tab active when it started, or nowhere; no `await` from bind to `set_current`.
    pub(super) fn open_file(
        &mut self,
        id: EntryId,
        kind: FileKind,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let origin = Origin {
            group: self.layout.last_center(),
            since: self.tree_cursor(cx),
        };
        let tab = self.group(origin.group).tabs.active();
        let opening = self.opening(id, kind, cx);
        // Dropped with its group, so the group it began in still exists when it lands.
        self.group_mut(origin.group).activation = Some(cx.spawn(async move |shell, cx| {
            let Some(landing) = opening.landed().await else {
                return;
            };
            let _ = shell.update_in(cx, |shell, window, cx| {
                let Group { tabs, shown, .. } = shell.group_mut(origin.group);
                // T4: `show` drops `activation` on every other change to what the active tab shows.
                if !tabs.commit(tab, || capture(shown, cx)) {
                    return;
                }
                shell.land(origin, id, landing, at, window, cx);
            });
        }));
    }

    /// Focus moves only if the group is still `last_center`: a switch since the open keeps it where the user put it.
    pub(super) fn land(
        &mut self,
        origin: Origin,
        id: EntryId,
        landing: Landing,
        at: Option<Range<ByteOffset>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.show_landed(origin.group, id, landing, None, origin.since, cx);
        if let (Some(editor), Some(range)) = (editor, at) {
            editor.update(cx, |editor, cx| editor.select_range(range, window, cx));
        }
        if origin.group == self.layout.last_center()
            && let Some(focus) = self.land_focus(cx)
        {
            window.focus(&focus, cx);
        }
    }

    /// The one entry for an opener that carries a target.
    pub(super) fn open(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        open: Open,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.file_kind(id, cx) else {
            return;
        };
        match open {
            // A view's tab keeps its view: a commit would push it into the tab's history.
            Open::Here if self.center().shown.view().is_none() => {
                self.show_file(id, kind, at, cx);
            }
            Open::Here | Open::NewTab => self.open_in_new_tab(id, kind, at, cx),
            Open::SplitRight => {
                let shell = cx.weak_entity();
                // A split needs the window, which `open`'s callers do not hold.
                cx.defer(move |cx| {
                    let _ = shell.update_in(cx, |shell, window, cx| {
                        shell.open_in_split(id, kind, at, window, cx);
                    });
                });
            }
        }
    }

    /// `id` in a fresh group right of the active one, else in a new tab where a half would be too narrow.
    fn open_in_split(
        &mut self,
        id: EntryId,
        kind: FileKind,
        at: Option<Range<ByteOffset>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from = self.layout.last_center();
        if self
            .split_group(from, SplitDirection::Right, Place::Empty, window, cx)
            .is_some()
        {
            self.open_file(id, kind, at, cx);
        } else {
            self.open_in_new_tab(id, kind, at, cx);
        }
    }

    /// Opens `id`, or only selects `at` if it is showing; either way no slower activation may land after it.
    pub(super) fn show_file(
        &mut self,
        id: EntryId,
        kind: FileKind,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        if self.showing() == Some(id) {
            self.center_mut().activation = None;
            if let (Some(range), Some(editor)) = (at, self.editor()) {
                cx.with_window(cx.entity_id(), |window, cx| {
                    editor.update(cx, |editor, cx| editor.select_range(range, window, cx));
                });
            }
            // Focus lands on the note, as `open_note` moves it for another note.
            if let Some(focus) = self.land_focus(cx) {
                cx.with_window(cx.entity_id(), |window, cx| window.focus(&focus, cx));
            }
        } else {
            self.open_file(id, kind, at, cx);
        }
    }

    /// The note's handle to focus now, or the tree's if the active group shows Files; `None` while a
    /// picker keeps the keys typed into it, until its dismissal lands there.
    pub(super) fn land_focus(&mut self, cx: &App) -> Option<FocusHandle> {
        let tree = (self.center().shown.view() == Some(View::Files))
            .then(|| self.tree_focus(cx))
            .flatten();
        let focus = tree.unwrap_or_else(|| self.focus_handle(cx));
        match &mut self.modal {
            Some(modal) => {
                modal.restore_to(focus);
                None
            }
            None => Some(focus),
        }
    }

    /// Moves focus to the note only if the root holds it, or a picker returns to it: it stays where the user put it.
    pub(super) fn focus_note_from_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = &self.focus_handle;
        let took_root = root.is_focused(window)
            || self
                .modal
                .as_ref()
                .is_some_and(|modal| modal.restores(root));
        if took_root && let Some(focus) = self.land_focus(cx) {
            window.focus(&focus, cx);
        }
    }

    /// The one place `shown` changes; only `last_center`'s change reaches the note's followers.
    pub(super) fn set_shown(
        &mut self,
        group: GroupId,
        shown: Shown,
        since: Cursor,
        cx: &mut Context<Self>,
    ) {
        let left = std::mem::replace(&mut self.group_mut(group).shown, shown);
        // A bound editor is only ever rebound, so an editor left here drops: it must not drop pinned.
        if let Shown::Bound(left) = left
            && self.editor_of(group).is_none()
        {
            left.editor.update(cx, EditorView::unpin);
        }
        if group == self.layout.last_center() {
            self.follow_center(since, cx);
        }
        self.save_session_later(cx);
        Self::release_images_later(cx);
    }

    /// `last_center`'s shown reaches the note's followers and `current`; the highlight moves only if the
    /// tree's cursor is still `since`. A view leaves `current` alone.
    pub(super) fn follow_center(&mut self, since: Cursor, cx: &mut Context<Self>) {
        let note = self.bound_note();
        if let Some(id) = note {
            remember(&mut self.recent, id);
        }
        if let Some(panes) = &self.panes {
            let file = self.showing();
            // No file shown keeps the highlight: a tree trash has already moved it to the neighbour.
            if let Some(id) = file {
                panes
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.follow(id, since, cx));
            }
            panes
                .backlinks
                .update(cx, |backlinks, cx| backlinks.set_file(file, cx));
        }
        match (&self.center().shown, note) {
            (Shown::View(_), _) => {}
            (_, Some(id)) => self.notes.update(cx, |notes, cx| notes.set_current(id, cx)),
            (_, None) => self.notes.update(cx, NoteStore::clear_current),
        }
    }

    /// Detached: the editor is created once and rebound, so it lives as long as the shell.
    /// Its notify is a caret move or an edit, which the session saves.
    pub(super) fn watch_editor(editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        cx.subscribe(editor, |shell, _, event: &LinkFollowed, cx| {
            shell.follow(&event.link, event.open, cx);
        })
        .detach();
        cx.observe(editor, |shell, _, cx| shell.save_session_later(cx))
            .detach();
    }

    /// Resolved against the note showing, before any tab changes; a detached note resolves only URLs.
    pub(super) fn follow(&mut self, link: &Link, open: Open, cx: &mut Context<Self>) {
        let resolution = match &self.panes {
            Some(panes) => panes
                .links
                .read(cx)
                .index()
                .resolve_link(self.bound_note(), link),
            None => match Target::parse(link) {
                Target::External(url) => Resolution::External(url),
                _ => Resolution::Unresolved,
            },
        };
        match resolution {
            Resolution::File(id) => self.open(id, None, open, cx),
            Resolution::External(url) if may_open(&url) => cx.open_url(&url),
            Resolution::External(url) => self.show_notice(
                format!(
                    "Didn't open \u{201c}{url}\u{201d}: only http, https and mailto links open"
                ),
                None,
                cx,
            ),
            Resolution::Unresolved | Resolution::SelfLink => {}
        }
    }

    /// Binds `id` into the group's editor, then `showing` and `current` if it is `last_center`, with no `await` between.
    pub(super) fn bind(
        &mut self,
        group: GroupId,
        id: EntryId,
        buffer: Entity<Buffer>,
        view: Option<ViewState>,
        since: Cursor,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let mut view = view;
        let editor = match &self.group(group).shown {
            Shown::Bound(bound) if bound.note == id => bound.editor.clone(),
            // A rebind restores the view itself, clipped to the new buffer.
            Shown::Bound(bound) => {
                let editor = bound.editor.clone();
                editor.update(cx, |editor, cx| editor.open_buffer(buffer, view.take(), cx));
                editor
            }
            Shown::Empty | Shown::Image(_) | Shown::Trashing(_) | Shown::View(_) => {
                let editor = cx.new(|cx| EditorView::new(buffer, cx));
                Self::watch_editor(&editor, cx);
                if group != self.layout.last_center() {
                    editor.update(cx, EditorView::pin);
                }
                editor
            }
        };
        if let Some(view) = view {
            editor.update(cx, |editor, cx| editor.restore(view, cx));
        }
        self.set_shown(
            group,
            Shown::Bound(Bound {
                note: id,
                editor: editor.clone(),
            }),
            since,
            cx,
        );
        editor
    }

    /// Before the editor drops: focus on a dropped handle reaches no action.
    pub(super) fn unfocus_editor(&self, group: GroupId, window: &mut Window, cx: &mut App) {
        if let Some(bound) = self.group(group).shown.bound()
            && bound.editor.focus_handle(cx).contains_focused(window, cx)
        {
            window.focus(&self.focus_handle, cx);
        }
    }
}

/// The only schemes a followed link hands to the system: a note is untrusted text.
const OPENED_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

fn may_open(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        OPENED_SCHEMES
            .iter()
            .any(|opened| scheme.eq_ignore_ascii_case(opened))
    })
}
