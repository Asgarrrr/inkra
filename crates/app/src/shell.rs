use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{App, Context, Entity, FocusHandle, Focusable, Pixels, Rgba, Window, px};
use inkra_vault::{EntryId, New};

use crate::editor::EditorView;
use crate::open::FileActivated;
use crate::quick_open::{QuickOpen, QuickOpenDelegate};
use crate::restore::Opened;
use crate::sidebar::{Cursor, NewFolder, NewNote};
use crate::theme::{self, store::ThemeStore};
use crate::vault::{ConfigChanged, GoneChanged, NoteClosed, NoteStore, Save, TrashFailed};
use crate::vault_search::{HitActivated, SearchVault, VaultSearchDelegate};

mod activation;
mod closing;
mod column;
mod dock_chrome;
mod dock_drag;
mod docks;
mod geometry;
mod group;
mod members;
mod modal;
mod nav;
mod notice;
mod palette;
mod panes;
mod parked;
mod quit;
mod render;
mod separator;
mod session;
mod split;
mod surface;
mod tab_bar;
mod tab_moves;
mod tabs;
mod themes;
mod title_bar;
mod workspace;

use group::Group;
use inkra_layout::{GroupId, Layout, View};
use modal::Modal;
use notice::{Notice, reason};
use panes::VaultPanes;
use session::{Restore, SessionFile};

pub(crate) use docks::{ToggleLeftDock, ToggleRightDock};
pub(crate) use split::{
    ActivatePaneDown, ActivatePaneLeft, ActivatePaneRight, ActivatePaneUp, SplitDown, SplitLeft,
    SplitRight, SplitUp,
};
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
    Image(EntryId),
    /// The note's trash is pending: no editor until `NoteClosed` or `TrashFailed`.
    /// Entered only from a bound note, so a failed trash always finds its buffer again.
    Trashing(EntryId),
    View(View),
}

impl Shown {
    fn bound(&self) -> Option<&Bound> {
        match self {
            Self::Bound(bound) => Some(bound),
            Self::Empty | Self::Image(_) | Self::Trashing(_) | Self::View(_) => None,
        }
    }

    /// The file the tab is on: a note bound or being trashed, or an image.
    fn file(&self) -> Option<EntryId> {
        match self {
            Self::Empty | Self::View(_) => None,
            Self::Bound(Bound { note: file, .. }) | Self::Image(file) | Self::Trashing(file) => {
                Some(*file)
            }
        }
    }

    fn view(&self) -> Option<View> {
        match self {
            Self::View(view) => Some(*view),
            Self::Empty | Self::Bound(_) | Self::Image(_) | Self::Trashing(_) => None,
        }
    }
}

pub(crate) struct AppShell {
    /// Where each group sits; written only by `change_layout`, and by the launch.
    layout: Layout,
    /// One per group of `layout`, with no other key.
    groups: HashMap<GroupId, Group>,
    notes: Entity<NoteStore>,
    /// `None` with no vault: the launch settles the mode once.
    panes: Option<VaultPanes>,
    /// Root of the dispatch tree while no editor exists — load-bearing, see `render`.
    focus_handle: FocusHandle,
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
    /// A left press on a history arrow, held or having opened the menu.
    press: Option<nav::Press>,
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
        cache: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (file, read) = session
            .filter(|_| notes.read(cx).vault().is_some())
            .map_or((None, None), |Opened { file, session }| {
                (Some(file), session)
            });
        let (layout, mut groups, pending) = Self::restored_groups(read);
        let panes = notes
            .read(cx)
            .vault()
            .cloned()
            .map(|vault| VaultPanes::new(vault, &notes, cache, window, cx));
        if let Some(editor) = &editor {
            Self::watch_editor(editor, cx);
        }
        cx.subscribe(&notes, |shell, _notes, event: &GoneChanged, cx| {
            let shows = |&group: &GroupId| {
                let bound = shell.group(group).shown.bound();
                bound.is_some_and(|bound| bound.note == event.0)
            };
            if shell.layout.center_groups().iter().any(shows) {
                cx.notify();
            }
        })
        .detach();
        // A pinned editor must not drop pinned (`EditorView`'s drop), and the shell's editors drop with it.
        cx.on_release(|shell, cx| {
            for group in shell.layout.center_groups() {
                if let Some(editor) = shell.editor_of(group) {
                    editor.update(cx, EditorView::unpin);
                }
            }
        })
        .detach();
        cx.subscribe_in(
            &notes,
            window,
            |shell, _notes, event: &NoteClosed, window, cx| shell.file_closed(event.0, window, cx),
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
        let (session, restore) = Self::open_session(file, pending, &notes, cx);
        if let Some((editor, note)) = editor.zip(first) {
            // `root` adopts the note it launches with, so its editor comes with `current`.
            groups.insert(
                layout.last_center(),
                Group {
                    shown: Shown::Bound(Bound { note, editor }),
                    ..Group::new()
                },
            );
        }
        let shell = Self {
            layout,
            groups,
            notes,
            panes,
            focus_handle: cx.focus_handle(),
            modal: None,
            recent: Vec::new(),
            recent_commands: Vec::new(),
            session,
            restore,
            title_drag_armed: false,
            press: None,
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
        };
        if cfg!(debug_assertions) {
            shell.check_layout(cx);
        }
        shell
    }

    #[cfg(feature = "shot")]
    pub(crate) fn set_backdrop(&mut self, backdrop: Rgba) {
        self.backdrop = Some(backdrop);
    }

    /// # Panics
    /// If `id` is not a group of the layout.
    fn group(&self, id: GroupId) -> &Group {
        let Some(group) = self.groups.get(&id) else {
            panic!("{id:?} has no group");
        };
        group
    }

    /// # Panics
    /// If `id` is not a group of the layout.
    fn group_mut(&mut self, id: GroupId) -> &mut Group {
        let Some(group) = self.groups.get_mut(&id) else {
            panic!("{id:?} has no group");
        };
        group
    }

    /// `last_center`'s group, the one the note consumers read.
    fn center(&self) -> &Group {
        self.group(self.layout.last_center())
    }

    fn center_mut(&mut self) -> &mut Group {
        self.group_mut(self.layout.last_center())
    }

    /// The file the center tab draws: a bound note or an image.
    pub(super) fn showing(&self) -> Option<EntryId> {
        match &self.center().shown {
            Shown::Bound(Bound { note: file, .. }) | Shown::Image(file) => Some(*file),
            Shown::Empty | Shown::Trashing(_) | Shown::View(_) => None,
        }
    }

    /// The note bound in the editor; `None` on a tab whose note is being trashed.
    pub(super) fn bound_note(&self) -> Option<EntryId> {
        self.center().shown.bound().map(|bound| bound.note)
    }

    /// Read when an open or a tab op begins; a synchronous op reads it in its own update.
    pub(super) fn tree_cursor(&self, cx: &App) -> Cursor {
        self.panes
            .as_ref()
            .map_or_else(Cursor::default, |panes| panes.sidebar.read(cx).cursor())
    }

    pub(super) fn editor(&self) -> Option<&Entity<EditorView>> {
        self.editor_of(self.layout.last_center())
    }

    pub(super) fn editor_of(&self, group: GroupId) -> Option<&Entity<EditorView>> {
        self.group(group).shown.bound().map(|bound| &bound.editor)
    }

    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes.update(cx, NoteStore::save_current);
    }

    /// Handled here, not in the tree, so it fires with the editor focused.
    fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        self.create(New::Note, window, cx);
    }

    fn new_folder(&mut self, _: &NewFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.create(New::Dir, window, cx);
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
            |shell, event: &FileActivated, cx| shell.open(event.file, None, event.open, cx),
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
    fn create(&mut self, new: New, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal(View::Files, window, cx);
        if let Some(panes) = &self.panes {
            panes.sidebar.update(cx, |sidebar, cx| {
                sidebar.start_creating(new, window, cx);
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
        self.center()
            .tab_scroll
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
