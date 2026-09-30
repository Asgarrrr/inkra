//! Opening a note into the editor, and following a link from it.

use std::ops::Range;

use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable, Window};
use inkra_index::Resolution;
use inkra_markdown::Link;
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::EntryId;

use super::tabs::capture;
use super::{AppShell, Bound, Shown, remember};
use crate::editor::{EditorView, LinkFollowed, ViewState};
use crate::open::Open;
use crate::sidebar::Cursor;

impl AppShell {
    /// Commits into the tab active when it started, or nowhere; no `await` from bind to `set_current`.
    pub(super) fn open_note(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let (tab, since) = (self.tabs.active(), self.tree_cursor(cx));
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                // T4: `show` drops `activation` on every other change to what the active tab shows.
                if !shell.tabs.commit(tab, || capture(&shell.shown, cx)) {
                    return;
                }
                shell.land(id, buffer, at, since, window, cx);
            });
        }));
    }

    pub(super) fn land(
        &mut self,
        id: EntryId,
        buffer: Entity<Buffer>,
        at: Option<Range<ByteOffset>>,
        since: Cursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.bind(id, buffer, None, since, cx);
        if let Some(range) = at {
            editor.update(cx, |editor, cx| editor.select_range(range, window, cx));
        }
        if let Some(focus) = self.land_focus(cx) {
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
        match open {
            Open::Here => self.show_note(id, at, cx),
            Open::NewTab => self.open_in_new_tab(id, at, cx),
        }
    }

    /// Opens `id`, or only selects `at` if it is showing; either way no slower activation may land after it.
    pub(super) fn show_note(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        if self.showing() == Some(id) {
            self.activation = None;
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
            self.open_note(id, at, cx);
        }
    }

    /// The note's handle to focus now; `None` while a picker keeps the keys typed into it, until its dismissal lands there.
    pub(super) fn land_focus(&mut self, cx: &App) -> Option<FocusHandle> {
        let focus = self.focus_handle(cx);
        match &mut self.modal {
            Some(modal) => {
                modal.restore_to(focus);
                None
            }
            None => Some(focus),
        }
    }

    /// The one place `shown` changes; the highlight moves only if the tree's cursor is still `since`.
    pub(super) fn set_shown(&mut self, shown: Shown, since: Cursor, cx: &mut Context<Self>) {
        self.shown = shown;
        let note = self.showing();
        // No bound note keeps the highlight: a tree trash has already moved it to the neighbour.
        if let Some(id) = note {
            remember(&mut self.recent, id);
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.follow(id, since, cx));
        }
        self.backlinks
            .update(cx, |backlinks, cx| backlinks.set_note(note, cx));
    }

    /// Detached: the editor is created once and rebound, so it lives as long as the shell.
    pub(super) fn follow_links_of(editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        cx.subscribe(editor, |shell, _, event: &LinkFollowed, cx| {
            shell.follow(&event.link, event.open, cx);
        })
        .detach();
    }

    /// Resolved against the note showing, before any tab changes; a detached note resolves only URLs.
    pub(super) fn follow(&mut self, link: &Link, open: Open, cx: &mut Context<Self>) {
        let source = self.showing().unwrap_or(EntryId::DETACHED);
        match self.links.read(cx).index().resolve_link(source, link) {
            Resolution::Note(id) => self.open(id, None, open, cx),
            Resolution::External(url) => cx.open_url(&url),
            Resolution::Unresolved | Resolution::SelfLink => {}
        }
    }

    /// Binds `id` into the editor, then `showing` and `current`, with no `await` between.
    pub(super) fn bind(
        &mut self,
        id: EntryId,
        buffer: Entity<Buffer>,
        view: Option<ViewState>,
        since: Cursor,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let mut view = view;
        let editor = match &self.shown {
            Shown::Bound(bound) if bound.note == id => bound.editor.clone(),
            // A rebind restores the view itself, clipped to the new buffer.
            Shown::Bound(bound) => {
                let editor = bound.editor.clone();
                editor.update(cx, |editor, cx| editor.open_buffer(buffer, view.take(), cx));
                editor
            }
            Shown::Empty | Shown::Trashing(_) => {
                let editor = cx.new(|cx| EditorView::new(buffer, cx));
                Self::follow_links_of(&editor, cx);
                editor
            }
        };
        if let Some(view) = view {
            editor.update(cx, |editor, cx| editor.restore(view, cx));
        }
        self.set_shown(
            Shown::Bound(Bound {
                note: id,
                editor: editor.clone(),
            }),
            since,
            cx,
        );
        self.notes.update(cx, |notes, cx| notes.set_current(id, cx));
        editor
    }

    /// Before the editor drops: focus on a dropped handle reaches no action.
    pub(super) fn unfocus_editor(&self, window: &mut Window, cx: &mut App) {
        if let Some(editor) = self.editor()
            && editor.focus_handle(cx).contains_focused(window, cx)
        {
            window.focus(&self.focus_handle, cx);
        }
    }
}
