use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable, Pixels, Task, Window, px};
use inkra_vault::{EntryId, Kind};

use crate::backlinks::BacklinksPanel;
use crate::editor::EditorView;
use crate::index::IndexSync;
use crate::quick_open::{QuickOpen, QuickOpenDelegate};
use crate::sidebar::{NewFolder, NewNote, NoteActivated, Sidebar};
use crate::vault::{GoneChanged, NoteClosed, NoteStore, Save, SaveFailed};
use crate::vault_search::{HitActivated, SearchVault, VaultSearchDelegate};

mod activation;
mod modal;
mod palette;
mod render;
mod tabs;

use inkra_tabs::Tabs;
use modal::Modal;
use tabs::Place;

pub(crate) use tabs::{ActivateLastTab, ActivateTab, CloseTab, NewTab, NextTab, PreviousTab};

const SIDEBAR_WIDTH: Pixels = px(220.);
const BACKLINKS_WIDTH: Pixels = px(260.);
/// The quick-open MRU (search spec D6) and the palette's recent commands (palette spec D3).
const RECENT: usize = 20;

/// Newest first, no repeat, at most `RECENT`.
fn remember<T: PartialEq>(recent: &mut Vec<T>, item: T) {
    recent.retain(|seen| *seen != item);
    recent.insert(0, item);
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
    /// The active tab's note, written only by `set_showing`. Not `NoteStore::current`: `trash` clears that before `NoteClosed`.
    showing: Option<EntryId>,
    tabs: Tabs<Place>,
    /// Held, never detached: dropping a `Task` cancels it, so a superseded
    /// load cannot rebind the editor behind a later activation.
    activation: Option<Task<()>>,
    /// The picker over the window, if any.
    modal: Option<Modal>,
    /// The notes shown, newest first; see `remember`.
    recent: Vec<EntryId>,
    /// The commands run from the palette, by action name, newest first.
    recent_commands: Vec<&'static str>,
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
            shell.show_note(event.0, None, cx);
        })
        .detach();
        if let Some(editor) = &editor {
            Self::follow_links_of(editor, cx);
        }
        cx.subscribe(&sidebar, |shell, _sidebar, event: &NoteActivated, cx| {
            shell.open_note(event.0, None, cx);
        })
        .detach();
        cx.subscribe(&notes, |_shell, _notes, event: &SaveFailed, _cx| {
            eprintln!("save failed for {:?}", event.0);
        })
        .detach();
        cx.subscribe(&notes, |shell, _notes, event: &GoneChanged, cx| {
            if shell.showing == Some(event.0) {
                cx.notify();
            }
        })
        .detach();
        // Drop the view, not leave it: a buffer with no file is still typable
        // and every keystroke would be lost.
        cx.subscribe(&notes, |shell, _notes, event: &NoteClosed, cx| {
            if shell.tabs.forget(|place| place.note == Some(event.0)) {
                cx.with_window(cx.entity_id(), |window, cx| shell.drop_editor(window, cx));
                shell.set_showing(None, cx);
            }
            cx.notify();
        })
        .detach();
        let first = Place {
            note: notes.read(cx).current(),
            view: None,
        };
        Self {
            editor,
            showing: first.note,
            tabs: Tabs::new(first),
            notes,
            sidebar,
            links,
            backlinks,
            focus_handle: cx.focus_handle(),
            activation: None,
            modal: None,
            recent: Vec::new(),
            recent_commands: Vec::new(),
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
        self.toggle_picker_with(
            window,
            cx,
            |cx| QuickOpenDelegate::new(&links, showing, recent, cx),
            |shell, event: &NoteActivated, cx| shell.show_note(event.0, None, cx),
        );
    }

    fn search_vault(&mut self, _: &SearchVault, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            return;
        };
        let links = self.links.clone();
        self.toggle_picker_with(
            window,
            cx,
            |cx| VaultSearchDelegate::new(&links, vault, cx),
            |shell, event: &HitActivated, cx| {
                shell.show_note(event.note, Some(event.range.clone()), cx);
            },
        );
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

#[cfg(test)]
mod tests;
