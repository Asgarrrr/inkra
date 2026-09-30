use std::path::PathBuf;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, Pixels, ScrollHandle, Task, Window,
    px,
};
use inkra_vault::{EntryId, Kind};

use crate::backlinks::BacklinksPanel;
use crate::editor::EditorView;
use crate::index::IndexSync;
use crate::open::NoteActivated;
use crate::quick_open::{QuickOpen, QuickOpenDelegate};
use crate::sidebar::{Cursor, NewFolder, NewNote, Sidebar};
use crate::vault::{GoneChanged, NoteClosed, NoteStore, Save, SaveFailed, TrashFailed};
use crate::vault_search::{HitActivated, SearchVault, VaultSearchDelegate};

mod activation;
mod modal;
mod palette;
mod parked;
mod render;
mod session;
mod tab_bar;
mod tabs;

use inkra_tabs::Tabs;
use modal::Modal;
use parked::Parked;
use session::{Restore, SessionFile};
use tabs::Place;

pub(crate) use tabs::{
    ActivateLastTab, ActivateTab, CloseTab, GoBack, GoForward, NewTab, NextTab, PreviousTab,
    ReopenClosedTab,
};

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

/// The editor and the note it shows, in one field: an editor never shows no note (T2).
struct Bound {
    note: EntryId,
    editor: Entity<EditorView>,
}

/// What the active tab shows; only `Bound` has an editor (T2).
enum Shown {
    Empty,
    Bound(Bound),
    /// The note's trash is pending: no editor until `NoteClosed` or `TrashFailed`.
    /// Entered only from a bound note, so a failed trash always finds its buffer again.
    Trashing(EntryId),
}

impl Shown {
    fn bound(&self) -> Option<&Bound> {
        match self {
            Self::Bound(bound) => Some(bound),
            Self::Empty | Self::Trashing(_) => None,
        }
    }

    /// The note the tab is on, bound or being trashed.
    fn note(&self) -> Option<EntryId> {
        match self {
            Self::Empty => None,
            Self::Bound(Bound { note, .. }) | Self::Trashing(note) => Some(*note),
        }
    }
}

pub(crate) struct AppShell {
    /// Written only by `set_shown`. Not `NoteStore::current`, which `trash` clears first.
    shown: Shown,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
    links: Entity<IndexSync>,
    backlinks: Entity<BacklinksPanel>,
    /// Root of the dispatch tree while no editor exists — load-bearing, see `render`.
    focus_handle: FocusHandle,
    tabs: Tabs<Place>,
    /// The views of inactive tabs showing a note; changed only by `park`, `unpark` and `drop_note`.
    parked: Parked,
    /// The bar's horizontal scroll, moved to the active tab on every switch.
    tab_scroll: ScrollHandle,
    /// Held, never detached: dropping a `Task` cancels it, so a superseded
    /// load cannot rebind the editor behind a later activation.
    activation: Option<Task<()>>,
    /// The picker over the window, if any.
    modal: Option<Modal>,
    /// The notes shown, newest first; see `remember`.
    recent: Vec<EntryId>,
    /// The commands run from the palette, by action name, newest first.
    recent_commands: Vec<&'static str>,
    /// `None` unless a vault launched with a session file: nothing else writes one.
    session: Option<SessionFile>,
    restore: Restore,
}

impl AppShell {
    pub(crate) fn new(
        editor: Option<Entity<EditorView>>,
        notes: Entity<NoteStore>,
        session: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), cx));
        let links = cx.new(|cx| IndexSync::new(&notes, cx));
        let backlinks = cx.new(|cx| BacklinksPanel::new(links.clone(), notes.clone(), cx));
        // A note linking to itself lists itself.
        cx.subscribe(&backlinks, |shell, _, event: &NoteActivated, cx| {
            shell.open(event.note, None, event.open, cx);
        })
        .detach();
        if let Some(editor) = &editor {
            Self::watch_editor(editor, cx);
        }
        cx.subscribe(&sidebar, |shell, _sidebar, event: &NoteActivated, cx| {
            shell.open(event.note, None, event.open, cx);
        })
        .detach();
        cx.subscribe(&notes, |_shell, _notes, event: &SaveFailed, _cx| {
            eprintln!("save failed for {:?}", event.0);
        })
        .detach();
        cx.subscribe(&notes, |shell, _notes, event: &GoneChanged, cx| {
            if shell.showing() == Some(event.0) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(
            &notes,
            window,
            |shell, _notes, event: &NoteClosed, window, cx| shell.note_closed(event.0, window, cx),
        )
        .detach();
        cx.subscribe_in(
            &notes,
            window,
            |shell, _notes, event: &TrashFailed, window, cx| {
                shell.trash_failed(event.0, window, cx);
            },
        )
        .detach();
        // Tab titles and the saved session follow a rename, as the sidebar's rows do.
        if let Some(vault) = notes.read(cx).vault().cloned() {
            cx.observe(&vault, |shell, _, cx| {
                shell.save_session_later(cx);
                shell.try_restore(cx);
                cx.notify();
            })
            .detach();
        }
        let first = notes.read(cx).current();
        let (session, restore) = Self::open_session(session, &notes, cx);
        Self {
            // `root` adopts the note it launches with, so its editor comes with `current`.
            shown: editor.zip(first).map_or(Shown::Empty, |(editor, note)| {
                Shown::Bound(Bound { note, editor })
            }),
            tabs: Tabs::new(),
            parked: Parked::default(),
            tab_scroll: ScrollHandle::new(),
            notes,
            sidebar,
            links,
            backlinks,
            focus_handle: cx.focus_handle(),
            activation: None,
            modal: None,
            recent: Vec::new(),
            recent_commands: Vec::new(),
            session,
            restore,
        }
    }

    /// The note bound in the editor; `None` on a tab whose note is being trashed.
    pub(super) fn showing(&self) -> Option<EntryId> {
        self.shown.bound().map(|bound| bound.note)
    }

    /// The note the active tab is on, bound or being trashed.
    pub(super) fn active_note(&self) -> Option<EntryId> {
        self.shown.note()
    }

    /// Read when an open or a tab op begins; a synchronous op reads it in its own update.
    pub(super) fn tree_cursor(&self, cx: &App) -> Cursor {
        self.sidebar.read(cx).cursor()
    }

    pub(super) fn editor(&self) -> Option<&Entity<EditorView>> {
        self.shown.bound().map(|bound| &bound.editor)
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
        let (links, showing, recent) = (self.links.clone(), self.showing(), self.recent.clone());
        self.toggle_picker_with(
            window,
            cx,
            |cx| QuickOpenDelegate::new(&links, showing, recent, cx),
            |shell, event: &NoteActivated, cx| shell.open(event.note, None, event.open, cx),
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
                shell.open(event.note, Some(event.range.clone()), event.open, cx);
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

#[cfg(feature = "shot")]
impl AppShell {
    /// Where tab `ix` was drawn last, for a scene to press on; `None` before the bar is drawn.
    pub(crate) fn tab_center(&self, ix: usize) -> Option<gpui::Point<Pixels>> {
        self.tab_scroll
            .bounds_for_item(ix)
            .map(|bounds| bounds.center())
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor().map_or_else(
            || self.focus_handle.clone(),
            |editor| editor.focus_handle(cx),
        )
    }
}

#[cfg(test)]
mod tests;
