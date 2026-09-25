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
use crate::quick_open::{QuickOpen, QuickOpenDelegate};
use crate::sidebar::{NewFolder, NewNote, NoteActivated, Sidebar};
use crate::theme;
use crate::vault::{NoteClosed, NoteStore, Save, SaveFailed};

mod modal;

use modal::Modal;

const SIDEBAR_WIDTH: Pixels = px(220.);
const BACKLINKS_WIDTH: Pixels = px(260.);
/// The quick-open MRU (spec D6).
const RECENT: usize = 20;

/// Newest first, no repeat, at most `RECENT`.
fn remember(recent: &mut Vec<EntryId>, id: EntryId) {
    recent.retain(|&seen| seen != id);
    recent.insert(0, id);
    recent.truncate(RECENT);
}

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
    /// The picker over the window, if any.
    modal: Option<Modal>,
    /// The notes shown, newest first; see `remember`.
    recent: Vec<EntryId>,
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
        // A note linking to itself lists itself.
        cx.subscribe(&backlinks, |shell, _, event: &NoteActivated, cx| {
            shell.show_note(event.0, cx);
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
            if let Some(editor) = shell.editor.take() {
                // Focus left on a dropped view's handle reaches no action, not even the shell's.
                let root = shell.focus_handle.clone();
                cx.with_window(cx.entity_id(), |window, cx| {
                    if editor.focus_handle(cx).contains_focused(window, cx) {
                        window.focus(&root, cx);
                    }
                });
            }
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
            modal: None,
            recent: Vec::new(),
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
                // The picker keeps the keys typed into it; dismissing it lands on the note.
                match &mut shell.modal {
                    Some(modal) => modal.restore_to(focus),
                    None => window.focus(&focus, cx),
                }
                shell.set_showing(Some(id), cx);
                notes.update(cx, |notes, cx| notes.set_current(id, cx));
            });
        }));
    }

    /// Opens `id` unless it is showing: reopening would reset its caret and scroll. Either way
    /// it is the last pick, so a slower activation of another note must not land after it.
    fn show_note(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if self.showing == Some(id) {
            self.activation = None;
            // Focus lands on the note, as `open_note` moves it for another note.
            let focus = self.focus_handle(cx);
            match &mut self.modal {
                Some(modal) => modal.restore_to(focus),
                None => {
                    cx.with_window(cx.entity_id(), |window, cx| window.focus(&focus, cx));
                }
            }
        } else {
            self.open_note(id, cx);
        }
    }

    /// The one place `showing` changes, so the backlinks panel cannot lag it.
    fn set_showing(&mut self, note: Option<EntryId>, cx: &mut Context<Self>) {
        self.showing = note;
        if let Some(id) = note {
            remember(&mut self.recent, id);
        }
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
            Resolution::Note(id) => self.show_note(id, cx),
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

    fn quick_open(&mut self, _: &QuickOpen, window: &mut Window, cx: &mut Context<Self>) {
        if self.notes.read(cx).vault().is_none() {
            return;
        }
        let (links, showing, recent) = (self.links.clone(), self.showing, self.recent.clone());
        let Some(picker) = self.toggle_picker(window, cx, |cx| {
            QuickOpenDelegate::new(&links, showing, recent, cx)
        }) else {
            return;
        };
        let opened = cx.subscribe(&picker, |shell, _, event: &NoteActivated, cx| {
            shell.show_note(event.0, cx);
        });
        if let Some(modal) = &mut self.modal {
            modal.hold(opened);
        }
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
            .on_action(cx.listener(Self::quick_open))
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
            .children(self.render_modal(cx))
    }
}

#[cfg(test)]
mod tests;
