//! Opening a note into the editor, and following a link from it.

use std::ops::Range;

use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable};
use inkra_index::Resolution;
use inkra_markdown::Link;
use inkra_text::ByteOffset;
use inkra_vault::EntryId;

use super::{AppShell, remember};
use crate::editor::{EditorView, LinkFollowed};

impl AppShell {
    /// Bind, select, focus and `set_current` run with no `await` between them, so
    /// nothing can observe one without the others.
    pub(super) fn open_note(
        &mut self,
        id: EntryId,
        at: Option<Range<ByteOffset>>,
        cx: &mut Context<Self>,
    ) {
        let notes = self.notes.clone();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                let editor = if let Some(editor) = &shell.editor {
                    editor.update(cx, |editor, cx| editor.open_buffer(buffer, None, cx));
                    editor.clone()
                } else {
                    let editor = cx.new(|cx| EditorView::new(buffer, cx));
                    Self::follow_links_of(&editor, cx);
                    shell.editor = Some(editor.clone());
                    editor
                };
                if let Some(range) = at {
                    editor.update(cx, |editor, cx| editor.select_range(range, window, cx));
                }
                if let Some(focus) = shell.land_focus(cx) {
                    window.focus(&focus, cx);
                }
                shell.set_showing(Some(id), cx);
                notes.update(cx, |notes, cx| notes.set_current(id, cx));
            });
        }));
    }

    /// Opens `id` unless it is showing: reopening would reset its caret, scroll and undo stack.
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
    fn land_focus(&mut self, cx: &App) -> Option<FocusHandle> {
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
}
