use gpui::{
    App, Context, Entity, FocusHandle, Focusable, Pixels, Rgba, ScrollHandle, Task, Window, px,
};
use inkra_vault::{EntryId, Kind};

use crate::editor::EditorView;
use crate::open::NoteActivated;
use crate::quick_open::{QuickOpen, QuickOpenDelegate};
use crate::restore::{Opened, SavedDocks};
use crate::sidebar::{Cursor, NewFolder, NewNote};
use crate::theme::{self, store::ThemeStore};
use crate::vault::{ConfigChanged, GoneChanged, NoteClosed, NoteStore, Save, TrashFailed};
use crate::vault_search::{HitActivated, SearchVault, VaultSearchDelegate};

mod activation;
mod docks;
mod modal;
mod notice;
mod palette;
mod panes;
mod parked;
mod quit;
mod render;
mod session;
mod tab_bar;
mod tabs;
mod themes;
mod title_bar;

use inkra_layout::View;
use inkra_tabs::Tabs;
use modal::Modal;
use notice::{Notice, reason};
use panes::VaultPanes;
use parked::Parked;
use session::{Restore, SessionFile};
use tabs::Place;

pub(crate) use docks::{ToggleLeftDock, ToggleRightDock};
pub(crate) use tabs::{
    ActivateLastTab, ActivateTab, CloseTab, GoBack, GoForward, NewTab, NextTab, PreviousTab,
    ReopenClosedTab,
};

const LEFT_DOCK_WIDTH: Pixels = px(220.);
const RIGHT_DOCK_WIDTH: Pixels = px(260.);
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
    /// `None` with no vault: the launch settles the mode once.
    panes: Option<VaultPanes>,
    /// Root of the dispatch tree while no editor exists — load-bearing, see `render`.
    focus_handle: FocusHandle,
    tabs: Tabs<Place, Parked>,
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
    /// A left press on a title drag region, not yet moved or released (S3-D3).
    title_drag_armed: bool,
    /// A quit is under way: a second one does nothing.
    quitting: bool,
    /// The latest failure, until dismissed (quit-and-failures spec, Q9).
    notice: Option<Notice>,
    /// Painted under everything where the window has no blur: an off-screen shot.
    backdrop: Option<Rgba>,
    themes: ThemeStore,
}

impl AppShell {
    pub(crate) fn new(
        editor: Option<Entity<EditorView>>,
        notes: Entity<NoteStore>,
        session: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let docks = session
            .as_ref()
            .and_then(|opened| opened.session.as_ref())
            .and_then(|session| session.docks.as_ref())
            .map(SavedDocks::docks)
            .unwrap_or_default();
        let panes = notes
            .read(cx)
            .vault()
            .cloned()
            .map(|vault| VaultPanes::new(vault, &notes, docks, cx));
        if let Some(editor) = &editor {
            Self::watch_editor(editor, cx);
        }
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
        cx.subscribe(&notes, |shell, _notes, event: &ConfigChanged, cx| {
            shell.config_changed(&event.0, cx);
        })
        .detach();
        cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow_system(window.appearance(), cx);
        })
        .detach();
        Self::watch_failures(&notes, cx);
        Self::quit_on_close(window, cx);
        let first = notes.read(cx).current();
        let unwatched = notes.update(cx, |notes, _| notes.take_unwatched());
        let (session, restore) = Self::open_session(session, &notes, cx);
        Self {
            // `root` adopts the note it launches with, so its editor comes with `current`.
            shown: editor.zip(first).map_or(Shown::Empty, |(editor, note)| {
                Shown::Bound(Bound { note, editor })
            }),
            tabs: Tabs::new(),
            tab_scroll: ScrollHandle::new(),
            notes,
            panes,
            focus_handle: cx.focus_handle(),
            activation: None,
            modal: None,
            recent: Vec::new(),
            recent_commands: Vec::new(),
            session,
            restore,
            title_drag_armed: false,
            quitting: false,
            notice: unwatched.map(|error| Notice {
                text: format!(
                    "Inkra can't follow changes made outside the app: {}",
                    reason(&error)
                )
                .into(),
                cleared_by_save: None,
            }),
            backdrop: None,
            themes: ThemeStore::default(),
        }
    }

    #[cfg(feature = "shot")]
    pub(crate) fn set_backdrop(&mut self, backdrop: Rgba) {
        self.backdrop = Some(backdrop);
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
        self.panes
            .as_ref()
            .map_or_else(Cursor::default, |panes| panes.sidebar.read(cx).cursor())
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
        let Some(panes) = &self.panes else {
            return;
        };
        let (links, showing, recent) = (panes.links.clone(), self.showing(), self.recent.clone());
        self.toggle_picker_with(
            window,
            cx,
            |cx| QuickOpenDelegate::new(&links, showing, recent, cx),
            |shell, event: &NoteActivated, cx| shell.open(event.note, None, event.open, cx),
        );
    }

    fn search_vault(&mut self, _: &SearchVault, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panes) = &self.panes else {
            return;
        };
        let (links, vault) = (panes.links.clone(), panes.vault.clone());
        self.toggle_picker_with(
            window,
            cx,
            |cx| VaultSearchDelegate::new(&links, vault, cx),
            |shell, event: &HitActivated, cx| {
                shell.open(event.note, Some(event.range.clone()), event.open, cx);
            },
        );
    }

    /// Reveals the tree first: the name field it focuses must be drawn.
    fn create(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.change_docks(window, cx, |docks| docks.reveal(View::Files));
        if let Some(panes) = &self.panes {
            panes.sidebar.update(cx, |sidebar, cx| {
                sidebar.start_creating(kind, window, cx);
            });
        }
    }
}

#[cfg(any(test, feature = "shot"))]
impl AppShell {
    fn panes(&self) -> &VaultPanes {
        let Some(panes) = &self.panes else {
            panic!("a vault is open");
        };
        panes
    }

    pub(crate) fn tree_focus_handle(&self, cx: &App) -> FocusHandle {
        self.panes().sidebar.read(cx).focus_handle(cx)
    }

    /// Where tree row `ix` was drawn last; `None` before the tree is drawn.
    pub(crate) fn tree_row_center(&self, ix: f32, cx: &App) -> Option<gpui::Point<Pixels>> {
        self.panes().sidebar.read(cx).row_center(ix)
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

    /// Where quick open's row `ix` was drawn last; `None` when quick open is not drawn.
    pub(crate) fn quick_open_row_center(&self, ix: f32, cx: &App) -> Option<gpui::Point<Pixels>> {
        self.modal_picker::<QuickOpenDelegate>()?
            .read(cx)
            .row_center(ix)
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
