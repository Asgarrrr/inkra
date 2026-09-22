//! The window's root view: composes the sidebar and the editor.
//!
//! Named, opaque slots rather than an ad hoc `div` tree assembled at render
//! time — see `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`
//! for why, and what this deliberately leaves out (sidebar content, resize,
//! a status bar, multi-pane).

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Styled, Task, Window, div, px,
};
use inkra_vault::{EntryId, Kind};

use crate::editor::EditorView;
use crate::sidebar::{NewFolder, NewNote, NoteActivated, Sidebar};
use crate::theme;
use crate::vault::{NoteClosed, NoteStore, Save, SaveFailed};

/// Wide enough for a nested note name at the deepest indent the sidebar draws
/// before it starts truncating. Fixed rather than resizable: the drag handle is
/// deferred, with its trigger in the app-shell spec.
const SIDEBAR_WIDTH: Pixels = px(220.);

pub(crate) struct AppShell {
    /// `None` until a note is opened. A launch with no argument, or on a vault
    /// with nothing clicked yet, used to get a real editable `Buffer` that
    /// `NoteStore` never tracked: no observer, no autosave, no sidebar row, no
    /// quit flush, and `save_current` returning at its `None` guard. Everything
    /// typed into it was lost. There is now nothing to type into.
    editor: Option<Entity<EditorView>>,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
    /// Focused while no editor exists, and the root of the dispatch tree either
    /// way. The `track_focus` in `render` is load-bearing: without it this
    /// handle is in no dispatch node, and dispatch falls back to the anonymous
    /// view boundary *above* the div carrying `.on_action`
    /// (`gpui/src/window.rs:6244-6251`, `key_dispatch.rs:611-614`,
    /// `view.rs:462-476`), so `cmd-s` would never reach its handler.
    focus_handle: FocusHandle,
    /// Which note the editor is showing. `NoteStore::current` is the same fact,
    /// but a subscriber cannot ask it: `trash` clears `current` before the
    /// event it emits is delivered, and delivery waits for the outermost update
    /// to flush. Held here so a delete of some *other* note cannot blank an
    /// editor that was never showing it.
    showing: Option<EntryId>,
    /// Held, never detached: dropping a `Task` cancels it
    /// (`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`), so a
    /// superseded load cannot rebind the editor behind a later activation.
    /// Copying `save`'s `.detach()` here reintroduces that race silently.
    activation: Option<Task<()>>,
}

impl AppShell {
    /// `AppShell` composes the window; it does not decide what buffer opens in
    /// it.
    ///
    /// The sidebar is built here rather than passed in, for the same reason:
    /// it is part of the composition, and it needs nothing its two arguments do
    /// not already carry.
    ///
    /// `editor` is `Some` only for a launch on a named file, which is the one
    /// state that has a note before the window does.
    pub(crate) fn new(
        editor: Option<Entity<EditorView>>,
        notes: Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), cx));
        // `cx.subscribe`, not `subscribe_in`: the latter wants a `Window` this
        // has no access to, and `open_buffer` does not need one.
        //
        // Detached safely: `AppShell` owns the `Sidebar`, so the two share a
        // lifetime, and the callback holds `AppShell` weakly
        // (`gpui/src/app/context.rs:104`). Left neither bound nor detached it
        // would drop at the semicolon and unsubscribe immediately.
        cx.subscribe(&sidebar, |shell, _sidebar, event: &NoteActivated, cx| {
            shell.open_note(event.0, cx);
        })
        .detach();
        // The one place a failed write surfaces. It reaches stderr and no
        // further, which a released binary has nowhere to show — the status
        // line that fixes that is deferred, with its trigger in the vault spec.
        // What this buys today is that the fix has one body to replace rather
        // than two call sites that grew apart.
        cx.subscribe(&notes, |_shell, _notes, event: &SaveFailed, _cx| {
            eprintln!("save failed for {:?}", event.0);
        })
        .detach();
        // The note on screen was deleted out from under the editor. Dropping
        // the view rather than leaving it is the point: a buffer whose file is
        // gone is still typable, and every keystroke into it would be lost —
        // the same defect the unnamed launch buffer had. `render_empty` takes
        // the slot back.
        cx.subscribe(&notes, |shell, _notes, event: &NoteClosed, cx| {
            if shell.showing != Some(event.0) {
                return;
            }
            shell.editor = None;
            shell.showing = None;
            cx.notify();
        })
        .detach();
        Self {
            editor,
            notes,
            sidebar,
            focus_handle: cx.focus_handle(),
            showing: None,
            activation: None,
        }
    }

    /// Bind, focus and `set_current` run with no `await` between them, so
    /// nothing can observe one without the others. A failed load is swallowed:
    /// the spec's deferred status line would surface it here.
    ///
    /// `update_in` resolves a `Window` here with none passed in, and that is
    /// not an accident to be tidied away: every entity built by `cx.new` inside
    /// a window update is registered against that window at construction
    /// (`gpui/src/app.rs:2975`, `:1934-1946`), and `open_window` pushes that
    /// stack around `build_root_view` (`:1356-1359`) — which is where
    /// `bootstrap::root` builds this view. Threading a `&mut Window` through
    /// `root` would buy nothing: `spawn_in` resolves through the very same
    /// lookup (`app/async_context.rs:80-86`). What it would cost is a signature
    /// change to the seam both entry points share.
    fn open_note(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let notes = self.notes.clone();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |shell, cx| {
            let Some(buffer) = opened.await else { return };
            let _ = shell.update_in(cx, |shell, window, cx| {
                match &shell.editor {
                    Some(editor) => editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx)),
                    // Creates rather than rebinds: there is no previous note
                    // whose undo stack `open_buffer` would record this on, and
                    // `EditorView::new` already subscribes and starts with no
                    // cached layout.
                    None => shell.editor = Some(cx.new(|cx| EditorView::new(buffer, cx))),
                }
                // Through the trait, not `editor.focus_handle` — one answer to
                // "what is focused" cannot disagree with itself, and this is
                // what puts the `Focusable` branch below under test.
                let focus = shell.focus_handle(cx);
                window.focus(&focus, cx);
                // Recorded beside `set_current` and never apart from it: the
                // two answer the same question, and a `NoteClosed` subscriber
                // reads this one because the store has already cleared the
                // other by the time the event arrives.
                shell.showing = Some(id);
                notes.update(cx, |notes, cx| notes.set_current(id, cx));
            });
        }));
    }

    /// Nothing to hold: the write chain lives on the note, in the store, which
    /// outlives every save. `⌘S` only preempts the timer that chain is already
    /// waiting on.
    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes.update(cx, NoteStore::save_current);
    }

    /// Both create gestures land here rather than in the tree, because the
    /// binding is scoped to this context so it fires with the editor focused —
    /// and dispatch walks from the focused leaf up *its* ancestors
    /// (`gpui/src/window.rs:6270`). With the editor focused the sidebar is a
    /// sibling subtree, so a handler registered there would never be reached.
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
    /// The vault tree's focus handle.
    ///
    /// Not what `Focusable::focus_handle` returns — that one answers "where does
    /// the window put focus", which is the editor once a note is open. Reaching
    /// the tree is something a test or a scene asks for deliberately, because
    /// clicking a note row hands focus to the editor and the tree's own
    /// bindings stop resolving until something gives it back.
    pub(crate) fn tree_focus_handle(&self, cx: &App) -> FocusHandle {
        self.sidebar.read(cx).focus_handle(cx)
    }
}

impl Focusable for AppShell {
    /// The editor's when there is one, so `bootstrap::run`'s launch-time focus
    /// call lands on the note a named file opened. Its own otherwise, so that
    /// call still lands somewhere in the dispatch tree rather than nowhere.
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.as_ref().map_or_else(
            || self.focus_handle.clone(),
            |editor| editor.focus_handle(cx),
        )
    }
}

impl AppShell {
    /// What fills the editor's slot when no note is open.
    ///
    /// A method rather than a view: static text with no state, no events and no
    /// focus of its own. It takes no `track_focus` either — the root div's
    /// covers this subtree, which is how Zed's own empty pane is arranged
    /// (`workspace/src/pane.rs:4610-4649`, `:4432-4434`).
    ///
    /// Neither string may name an action this window cannot perform: the folder
    /// picker is deferred (`prompt_for_paths`, `gpui/src/app.rs:1687`) and
    /// creating a note needs the create/rename/delete slice. With a vault the
    /// sidebar is the affordance; without one, the only true instruction is to
    /// restart. "A folder path" rather than the literal `cargo run` invocation,
    /// which goes stale the day this ships as a binary.
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
            // It stands in the editor's slot, so it paints the editor's
            // surface. `EditorView` paints its own; without this the window
            // default shows through and the slot goes black beside a sidebar
            // that is not. `check.sh` renders nothing, so only a shot catches it.
            .bg(theme::editor_background())
            // Stated, never inherited: gpui's default line box is `phi()` on a
            // 16 px default, which is neither of this window's two scales.
            .text_size(theme::UI_TEXT_SIZE)
            .line_height(theme::UI_LINE_HEIGHT)
            .text_color(theme::placeholder_text())
            .children(lines.iter().map(|line| div().child(*line)))
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here rather than on `EditorView`: the sidebar will be
            // focusable, and a binding scoped to the editor would do nothing
            // while a note row holds focus. The action still reaches this
            // handler from the editor, because dispatch walks from the focused
            // node up to the root.
            .key_context("AppShell")
            // Load-bearing, not hygiene: with no editor the window's focus is
            // on `self.focus_handle`, and an untracked handle puts dispatch on
            // the view boundary above this div, where the action below is not
            // reachable. `cmd-s` would silently stop working with no note open.
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .flex()
            .flex_row()
            .size_full()
            .child(
                // flex_none (grow 0, shrink 0): never competes with the
                // editor for space.
                div()
                    .flex_none()
                    .w(SIDEBAR_WIDTH)
                    .h_full()
                    .bg(theme::sidebar_background())
                    .child(self.sidebar.clone()),
            )
            .child(
                // flex_1 (grow 1, shrink 1, basis 0%) makes this wrapper
                // claim all remaining row width — gpui/styled.rs:181-186.
                // EditorView's own .size_full() then fills 100% of this
                // wrapper's resolved size; EditorView never needs to know
                // it's no longer the window root.
                div().flex_1().h_full().child(match &self.editor {
                    Some(editor) => editor.clone().into_any_element(),
                    None => self.render_empty(cx).into_any_element(),
                }),
            )
    }
}

#[cfg(test)]
mod tests;
