//! Opening a note into the editor, and following a link from it.

use std::ops::Range;

use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable, Window};
use inkra_index::Resolution;
use inkra_markdown::Link;
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::EntryId;

use super::tabs::{Place, capture};
use super::{AppShell, remember};
use crate::editor::{EditorView, LinkFollowed, ViewState};

impl AppShell {
    /// Commits into the tab active when it started, or nowhere; bind, select, focus and
    /// `set_current` run with no `await` between them.
    pub(super) fn open_note(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let tab = self.tabs.active();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                let place = Place {
                    note: Some(id),
                    view: None,
                };
                // Backed today by dropping `activation` on every tab op; load-bearing once S3 opens into other tabs.
                if !shell.tabs.commit(tab, place, || {
                    capture(shell.showing, shell.editor.as_ref(), cx)
                }) {
                    return;
                }
                let editor = shell.bind(id, buffer, None, cx);
                if let Some(range) = at {
                    editor.update(cx, |editor, cx| editor.select_range(range, window, cx));
                }
                if let Some(focus) = shell.land_focus(cx) {
                    window.focus(&focus, cx);
                }
            });
        }));
    }

    /// Opens `id` unless it is showing, which only selects `at` and takes focus, synchronously.
    /// Either way it is the last pick, so a slower activation of another note must not land after it.
    pub(super) fn show_note(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        if self.showing == Some(id) {
            self.activation = None;
            if let (Some(range), Some(editor)) = (at, &self.editor) {
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

    /// The one place `showing` changes, so the backlinks panel cannot lag it.
    pub(super) fn set_showing(&mut self, note: Option<EntryId>, cx: &mut Context<Self>) {
        self.showing = note;
        if let Some(id) = note {
            remember(&mut self.recent, id);
        }
        self.backlinks
            .update(cx, |backlinks, cx| backlinks.set_note(note, cx));
    }

    /// Detached, like the shell's other subscriptions: the editor is created once and rebound
    /// after, so it lives as long as the shell.
    pub(super) fn follow_links_of(editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        cx.subscribe(editor, |shell, _, event: &LinkFollowed, cx| {
            shell.follow(&event.0, cx);
        })
        .detach();
    }

    /// Resolved against the note showing; a detached note resolves only URLs.
    pub(super) fn follow(&mut self, link: &Link, cx: &mut Context<Self>) {
        let source = self.showing.unwrap_or(EntryId::DETACHED);
        match self.links.read(cx).index().resolve_link(source, link) {
            Resolution::Note(id) => self.show_note(id, None, cx),
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
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let editor = match &self.editor {
            Some(editor) if self.showing == Some(id) => {
                if let Some(view) = view {
                    editor.update(cx, |editor, cx| editor.restore(view, cx));
                }
                editor.clone()
            }
            Some(editor) => {
                editor.update(cx, |editor, cx| editor.open_buffer(buffer, view, cx));
                editor.clone()
            }
            None => {
                let editor = cx.new(|cx| EditorView::new(buffer, cx));
                if let Some(view) = view {
                    editor.update(cx, |editor, cx| editor.restore(view, cx));
                }
                Self::follow_links_of(&editor, cx);
                self.editor = Some(editor.clone());
                editor
            }
        };
        self.set_showing(Some(id), cx);
        self.notes.update(cx, |notes, cx| notes.set_current(id, cx));
        editor
    }

    /// Takes the window: an action holds it leased, so `with_window` would return `None` there.
    pub(super) fn drop_editor(&mut self, window: &mut Window, cx: &mut App) {
        if let Some(editor) = self.editor.take() {
            // Focus left on a dropped view's handle reaches no action, not even the shell's.
            if editor.focus_handle(cx).contains_focused(window, cx) {
                window.focus(&self.focus_handle, cx);
            }
        }
    }
}
