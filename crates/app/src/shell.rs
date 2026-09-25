use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Styled, Task, Window, div, px,
};
use inkra_index::Resolution;
use inkra_markdown::Link;
use inkra_vault::{EntryId, Kind};

use crate::backlinks::BacklinksPanel;
use crate::editor::{EditorView, LinkFollowed};
use crate::index::IndexSync;
use crate::sidebar::{NewFolder, NewNote, NoteActivated, Sidebar};
use crate::theme;
use crate::vault::{NoteClosed, NoteStore, Save, SaveFailed};

const SIDEBAR_WIDTH: Pixels = px(220.);
const BACKLINKS_WIDTH: Pixels = px(260.);

pub(crate) struct AppShell {
    /// Must not default to a real, untracked `Buffer` — see CLAUDE.md.
    editor: Option<Entity<EditorView>>,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
    links: Entity<IndexSync>,
    backlinks: Entity<BacklinksPanel>,
    /// Root of the dispatch tree while no editor exists — load-bearing, see `render`.
    focus_handle: FocusHandle,
    /// Not `NoteStore::current`: `trash` clears that before `NoteClosed` is delivered.
    showing: Option<EntryId>,
    /// Held, never detached: dropping a `Task` cancels it, so a superseded
    /// load cannot rebind the editor behind a later activation.
    activation: Option<Task<()>>,
}

impl AppShell {
    pub(crate) fn new(
        editor: Option<Entity<EditorView>>,
        notes: Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), cx));
        let links = cx.new(|cx| IndexSync::new(&notes, cx));
        let backlinks = cx.new(|cx| BacklinksPanel::new(links.clone(), notes.clone(), cx));
        // A note linking to itself lists itself; reopening it would reset its caret and scroll.
        cx.subscribe(&backlinks, |shell, _, event: &NoteActivated, cx| {
            if shell.showing != Some(event.0) {
                shell.open_note(event.0, cx);
            }
        })
        .detach();
        if let Some(editor) = &editor {
            Self::follow_links_of(editor, cx);
        }
        cx.subscribe(&sidebar, |shell, _sidebar, event: &NoteActivated, cx| {
            shell.open_note(event.0, cx);
        })
        .detach();
        cx.subscribe(&notes, |_shell, _notes, event: &SaveFailed, _cx| {
            eprintln!("save failed for {:?}", event.0);
        })
        .detach();
        // Drop the view, not leave it: a buffer with no file is still typable
        // and every keystroke would be lost.
        cx.subscribe(&notes, |shell, _notes, event: &NoteClosed, cx| {
            if shell.showing != Some(event.0) {
                return;
            }
            shell.editor = None;
            shell.set_showing(None, cx);
            cx.notify();
        })
        .detach();
        Self {
            editor,
            notes,
            sidebar,
            links,
            backlinks,
            focus_handle: cx.focus_handle(),
            showing: None,
            activation: None,
        }
    }

    /// Bind, focus and `set_current` run with no `await` between them, so
    /// nothing can observe one without the others.
    fn open_note(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let notes = self.notes.clone();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                if let Some(editor) = &shell.editor {
                    editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx));
                } else {
                    let editor = cx.new(|cx| EditorView::new(buffer, cx));
                    Self::follow_links_of(&editor, cx);
                    shell.editor = Some(editor);
                }
                let focus = shell.focus_handle(cx);
                window.focus(&focus, cx);
                shell.set_showing(Some(id), cx);
                notes.update(cx, |notes, cx| notes.set_current(id, cx));
            });
        }));
    }

    /// The one place `showing` changes, so the backlinks panel cannot lag it.
    fn set_showing(&mut self, note: Option<EntryId>, cx: &mut Context<Self>) {
        self.showing = note;
        self.backlinks
            .update(cx, |backlinks, cx| backlinks.set_note(note, cx));
    }

    /// Detached, like the shell's other subscriptions: the editor is created once and rebound
    /// after, so it lives as long as the shell.
    fn follow_links_of(editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        cx.subscribe(editor, |shell, _, event: &LinkFollowed, cx| {
            shell.follow(&event.0, cx);
        })
        .detach();
    }

    /// Resolved against the note showing; a detached note resolves only URLs.
    fn follow(&mut self, link: &Link, cx: &mut Context<Self>) {
        let source = self.showing.unwrap_or(EntryId::DETACHED);
        match self.links.read(cx).index().resolve_link(source, link) {
            // Rebinding the note showing would reset its caret and scroll.
            Resolution::Note(id) if self.showing == Some(id) => {}
            Resolution::Note(id) => self.open_note(id, cx),
            Resolution::External(url) => cx.open_url(&url),
            Resolution::Unresolved | Resolution::SelfLink => {}
        }
    }

    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes.update(cx, NoteStore::save_current);
    }

    /// Handled here, not in the tree, so it fires with the editor focused.
    fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        self.create(Kind::Note, window, cx);
    }

    fn new_folder(&mut self, _: &NewFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.create(Kind::Dir, window, cx);
    }

    fn create(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.start_creating(kind, window, cx);
        });
    }
}

#[cfg(any(test, feature = "shot"))]
impl AppShell {
    pub(crate) fn tree_focus_handle(&self, cx: &App) -> FocusHandle {
        self.sidebar.read(cx).focus_handle(cx)
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.as_ref().map_or_else(
            || self.focus_handle.clone(),
            |editor| editor.focus_handle(cx),
        )
    }
}

impl AppShell {
    fn render_empty(&self, cx: &App) -> impl IntoElement {
        let lines: &[&str] = if self.notes.read(cx).vault().is_some() {
            &["Select a note."]
        } else {
            &[
                "No vault is open.",
                "Restart with a folder path to open one.",
            ]
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            // Without this the window default shows through; check.sh renders nothing, so only a shot catches it.
            .bg(theme::editor_background())
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(theme::placeholder_text())
            .children(lines.iter().map(|line| div().child(*line)))
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here, not on `EditorView`: a note row can hold focus too, and dispatch walks up to the root.
            .key_context("AppShell")
            // Load-bearing: with no editor, focus lives on `self.focus_handle`; untracked, `cmd-s` is unreachable (gpui/src/window.rs:6244-6251).
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .flex_none()
                    .w(SIDEBAR_WIDTH)
                    .h_full()
                    .bg(theme::sidebar_background())
                    .child(self.sidebar.clone()),
            )
            .child(div().flex_1().h_full().child(match &self.editor {
                Some(editor) => editor.clone().into_any_element(),
                None => self.render_empty(cx).into_any_element(),
            }))
            .children(self.editor.is_some().then(|| {
                div()
                    .flex_none()
                    .w(BACKLINKS_WIDTH)
                    .h_full()
                    .child(self.backlinks.clone())
            }))
    }
}

#[cfg(test)]
mod tests;
