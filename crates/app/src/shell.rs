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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    /// A rename keeps the row's `EntryId`, which is what keeps whatever buffer
    /// is open under it. Asserted by id, not by name: the name is what moved.
    #[gpui::test]
    fn renaming_a_note_moves_it_and_keeps_its_id(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("down");
        visual.run_until_parked();
        let before = sidebar
            .read_with(cx, |sidebar, _| sidebar.selected())
            .unwrap();

        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        visual.simulate_keystrokes("p l a n enter");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["b.md", "plan.md"]);
        assert_eq!(
            entry_id(&notes, cx, "plan"),
            before,
            "the inode did not move, so neither did the id"
        );
    }

    /// The one hazard the chain exists for. A write is held open past the turn
    /// that issued it, the rename is issued behind it, and the gate is released:
    /// the write must land on the old path first and the rename must move that
    /// file, rather than the write recreating the note under its old name.
    ///
    /// Without the chain the rename races the write. `MemFs::block_save` is the
    /// only way to reach the interleaving at all.
    #[gpui::test]
    async fn a_rename_issued_during_a_write_moves_the_written_text(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
        let visual = tree_focused(cx, window, &sidebar);

        activate(&sidebar, cx, a);
        visual.run_until_parked();
        let buffer = notes
            .read_with(cx, |notes, _| notes.buffer_for(a).cloned())
            .expect("the note is open");

        // Hold the write open, then dirty the note so one is issued.
        let gate = fs.block_save("/vault/a.md");
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(std::time::Instant::now(), |buffer| {
                buffer.insert(inkra_text::ByteOffset(0), "EDITED ");
            });
        });
        notes.update(cx, NoteStore::save_current);
        visual.run_until_parked();

        // Issued while that write is still in flight.
        let renamed = notes.update(cx, |notes, cx| notes.rename(a, "plan", cx));
        visual.run_until_parked();
        gate.release();
        assert!(renamed.await, "the rename lands once the write has");
        visual.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/plan.md")).as_deref(),
            Some("EDITED alpha"),
            "the write reached disk, and the rename moved what it wrote"
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")),
            None,
            "nothing was recreated under the old name"
        );
    }

    /// The note stays writable afterwards, at its new path, under the same id.
    ///
    /// Does **not** pin the rescan-inside-the-link: removing that await leaves
    /// this green, because `run_until_parked` drains the walk before the edit.
    /// What it does pin is that the buffer survives the move and its next write
    /// follows the file — which is the user-visible half.
    #[gpui::test]
    async fn a_note_edited_after_a_rename_is_written_to_its_new_path(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
        let visual = tree_focused(cx, window, &sidebar);

        activate(&sidebar, cx, a);
        visual.run_until_parked();
        assert!(
            notes
                .update(cx, |notes, cx| notes.rename(a, "plan", cx))
                .await
        );
        visual.run_until_parked();

        let buffer = notes
            .read_with(cx, |notes, _| notes.buffer_for(a).cloned())
            .expect("the note is still open under the same id");
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(std::time::Instant::now(), |buffer| {
                buffer.insert(inkra_text::ByteOffset(0), "AFTER ");
            });
        });
        notes.update(cx, NoteStore::save_current);
        visual.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/plan.md")).as_deref(),
            Some("AFTER alpha")
        );
        assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
    }

    #[gpui::test]
    fn renaming_onto_a_sibling_name_is_refused(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("down enter");
        visual.run_until_parked();
        visual.simulate_keystrokes("b enter");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"], "neither moved");
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
            Some("That name is already taken.")
        );
    }

    /// Confirming a rename without changing anything is a no-op, not a
    /// collision with itself.
    #[gpui::test]
    fn renaming_a_note_to_its_own_name_is_accepted(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("down enter enter");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
        assert!(
            !sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
            "the field closed rather than reporting a duplicate"
        );
    }

    /// The field replaces the row rather than being inserted beside it, so a
    /// rename never shows the old name and the field for one entry at once.
    #[gpui::test]
    fn renaming_replaces_the_row_rather_than_adding_one(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, _notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        let before = sidebar.read_with(cx, |sidebar, _| sidebar.row_count());
        visual.simulate_keystrokes("down enter");
        visual.run_until_parked();

        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.row_count()),
            before
        );
    }

    #[gpui::test]
    fn renaming_a_folder_moves_its_subtree(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/projects/plan.md", "p");
        fs.insert_file("/vault/a.md", "alpha");
        let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let mut visual = tree_focused(cx, window, &sidebar);

        // Row 0 is the `projects` folder.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        visual.simulate_keystrokes("a r c h i v e enter");
        visual.run_until_parked();

        assert_eq!(
            vault_names(&notes, cx),
            ["archive", "archive/plan.md", "a.md"]
        );
    }

    /// Names the vault holds now, in tree order.
    fn vault_names(notes: &Entity<NoteStore>, cx: &mut TestAppContext) -> Vec<String> {
        notes.update(cx, |notes, cx| {
            notes
                .vault()
                .expect("a vault")
                .read(cx)
                .entries()
                .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
                .collect()
        })
    }

    /// `cmd-n` while the **editor** holds focus.
    ///
    /// The placement test, and the reason the handler is on `AppShell` rather
    /// than on the tree: dispatch walks from the focused leaf up its own
    /// ancestors, and with the editor focused the sidebar is a sibling subtree.
    /// A handler registered there is never reached, and the key goes quiet.
    #[gpui::test]
    fn cmd_n_opens_a_field_with_the_editor_focused(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        // Put focus on the editor, which is the state the tree cannot reach.
        let a = entry_id(&notes, cx, "a");
        activate(&sidebar, cx, a);
        visual.run_until_parked();
        window
            .update(cx, |shell, window, cx| {
                let handle = shell.focus_handle(cx);
                window.focus(&handle, cx);
            })
            .unwrap();
        visual.run_until_parked();

        visual.simulate_keystrokes("cmd-n");
        visual.run_until_parked();

        assert!(
            sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
            "cmd-n must reach the tree from the editor"
        );
    }

    #[gpui::test]
    fn typing_a_name_and_confirming_creates_the_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n");
        visual.run_until_parked();
        visual.simulate_keystrokes("p l a n enter");
        visual.run_until_parked();

        assert_eq!(
            vault_names(&notes, cx),
            ["a.md", "b.md", "plan.md"],
            "the note exists, with the extension the name rules add"
        );
        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
    }

    /// The created row is selected, so the next gesture acts on what was just
    /// made rather than on whatever was selected before.
    #[gpui::test]
    fn the_created_note_becomes_the_selection(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n p l a n enter");
        visual.run_until_parked();

        let made = entry_id(&notes, cx, "plan");
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
            Some(made)
        );
    }

    #[gpui::test]
    fn cmd_shift_n_creates_a_folder_rather_than_a_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-shift-n");
        visual.run_until_parked();
        visual.simulate_keystrokes("d o c s enter");
        visual.run_until_parked();

        assert_eq!(
            vault_names(&notes, cx),
            ["docs", "a.md", "b.md"],
            "a folder, with no extension, and sorted before the notes"
        );
    }

    /// Refused rather than silently doing nothing, and the field stays open so
    /// the name can be fixed. Zed's own confirm returns without a word here.
    #[gpui::test]
    fn a_duplicate_name_is_refused_and_the_field_stays_open(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n a enter");
        visual.run_until_parked();

        assert_eq!(
            vault_names(&notes, cx),
            ["a.md", "b.md"],
            "nothing was made"
        );
        assert!(
            sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
            "the field stays open so the name can be corrected"
        );
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
            Some("That name is already taken.")
        );
    }

    /// `plan` and `plan.md` are one name: the check compares the file name the
    /// note would actually get, not the text typed.
    #[gpui::test]
    fn a_duplicate_is_caught_through_the_extension(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n a . m d enter");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
    }

    #[gpui::test]
    fn an_invalid_name_is_refused_with_its_own_reason(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n a / b enter");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.rejection_message()),
            Some("A name cannot contain / or :.")
        );
    }

    #[gpui::test]
    fn escape_abandons_the_field_and_makes_nothing(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
        let mut visual = tree_focused(cx, window, &sidebar);

        visual.simulate_keystrokes("cmd-n p l a n escape");
        visual.run_until_parked();

        assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
    }

    /// A folder selected takes the new note inside it, and is expanded so the
    /// field is drawn somewhere visible.
    #[gpui::test]
    fn a_note_created_under_a_selected_folder_lands_inside_it(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/projects/existing.md", "e");
        fs.insert_file("/vault/a.md", "alpha");
        let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let mut visual = tree_focused(cx, window, &sidebar);

        // Row 0 is the `projects` folder.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();
        visual.simulate_keystrokes("cmd-n p l a n enter");
        visual.run_until_parked();

        assert!(
            vault_names(&notes, cx).contains(&"projects/plan.md".to_owned()),
            "got {:?}",
            vault_names(&notes, cx)
        );
    }

    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gpui::{
        Bounds, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
        WindowHandle, WindowOptions, point, prelude::*, px,
    };
    use inkra_text::Buffer;
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{AppShell, EditorView, EntryId, NoteActivated, NoteStore, Save, Sidebar};
    use crate::bootstrap::{Launch, root};
    use crate::keymap::key_bindings;
    use crate::sidebar::row_center;

    /// Rooted at `AppShell`, not at the `Sidebar`: that puts the sidebar under
    /// its real `SIDEBAR_WIDTH` parent, and it is the only way to reach the subscription
    /// `AppShell::new` sets up.
    /// No editor: a vault launch has none until a row is activated, which is the
    /// state this file exists to exercise. Reach it with [`editor`] afterwards.
    fn open_vault(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (WindowHandle<AppShell>, Entity<Sidebar>, Entity<NoteStore>) {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        let vault = cx.update(|cx| cx.new(|_| scanned));
        let notes = cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)));
        let window = cx.add_window({
            let notes = notes.clone();
            move |_, cx| AppShell::new(None, notes, cx)
        });
        let sidebar = window
            .update(cx, |shell, _, _| shell.sidebar.clone())
            .expect("the shell is open");
        (window, sidebar, notes)
    }

    /// The editor, once a note has been activated.
    ///
    /// Panics before that, and that is the contract: under an optional editor,
    /// "there is an editor" is something a test establishes rather than
    /// something it is handed. Reads the private field directly, as `open_vault`
    /// already does for the sidebar — this module is a descendant of `shell`.
    fn editor(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Entity<EditorView> {
        window
            .update(cx, |shell, _, _| shell.editor.clone())
            .expect("the shell is open")
            .expect("a note has been activated, so the editor exists")
    }

    /// By name, never by index: the comparator decides the order.
    fn entry_id(notes: &Entity<NoteStore>, cx: &mut TestAppContext, name: &str) -> EntryId {
        notes.update(cx, |notes, cx| {
            notes
                .vault()
                .expect("a vault")
                .read(cx)
                .entries()
                .find(|entry| entry.path.display_name() == name)
                .map(|entry| entry.id)
                .expect("the entry is in the vault")
        })
    }

    /// Returns the window rather than an editor: there is none yet, and its
    /// callers need the handle to ask [`editor`] for one after activating.
    fn two_notes(
        cx: &mut TestAppContext,
        fs: &Arc<MemFs>,
    ) -> (
        WindowHandle<AppShell>,
        Entity<Sidebar>,
        Entity<NoteStore>,
        EntryId,
        EntryId,
    ) {
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (window, sidebar, notes) = open_vault(cx, Arc::clone(fs) as Arc<dyn Fs>);
        let a = entry_id(&notes, cx, "a");
        let b = entry_id(&notes, cx, "b");
        (window, sidebar, notes, a, b)
    }

    /// Indistinguishable from a click to `AppShell`, and it needs no `Row`, so
    /// nothing in `sidebar.rs` widens for it.
    fn activate(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, id: EntryId) {
        sidebar.update(cx, |_, cx| cx.emit(NoteActivated(id)));
    }

    /// A real window with the bindings installed and the sidebar focused.
    ///
    /// The sidebar has to be *drawn* before it can be focused: `track_focus`
    /// only puts the handle in a dispatch node when the element renders.
    fn tree_focused(
        cx: &mut TestAppContext,
        window: WindowHandle<AppShell>,
        sidebar: &Entity<Sidebar>,
    ) -> VisualTestContext {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let visual = VisualTestContext::from_window(window.into(), cx);
        visual.run_until_parked();
        window
            .update(cx, |_, window, cx| {
                let handle = sidebar.read(cx).focus_handle(cx);
                window.focus(&handle, cx);
            })
            .expect("the shell is open");
        visual.run_until_parked();
        visual
    }

    /// `down` reaches the tree, and only the tree.
    ///
    /// Does **not** discriminate the `Some("EditorView")` scoping in
    /// `keymap.rs`, and says so rather than implying otherwise: with those
    /// bindings context-less the two entries tie and the tree's still wins, by
    /// sitting later in the `Vec` (`gpui/src/keymap.rs:188-190`). What this
    /// pins is the binding, the `Sidebar` key context reaching a dispatch node
    /// at all, and the focus the tree takes — delete any of the three and it
    /// fails.
    ///
    /// The caret assertion is belt and braces: the editor is not an ancestor of
    /// the sidebar, so an editor action dispatched from here could not reach it
    /// anyway. It costs one line and states the intent.
    #[gpui::test]
    fn an_arrow_moves_the_tree_and_not_the_caret_while_the_tree_has_focus(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, b) = two_notes(cx, &fs);
        activate(&sidebar, cx, a);
        cx.run_until_parked();
        let caret_before = editor(&window, cx).update(cx, |editor, _| editor.cursor());

        let mut visual = tree_focused(cx, window, &sidebar);
        visual.simulate_keystrokes("down down");

        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
            Some(b),
            "the tree must claim the arrow"
        );
        assert_eq!(
            editor(&window, cx).update(cx, |editor, _| editor.cursor()),
            caret_before,
            "and the editor must not also see it"
        );
        let _ = notes;
    }

    /// The binding, the prompt and the delete, end to end. A test that called
    /// `NoteStore::trash` directly would prove none of the three.
    #[gpui::test]
    fn cmd_backspace_asks_before_trashing_the_selected_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
        activate(&sidebar, cx, a);
        cx.run_until_parked();

        let mut visual = tree_focused(cx, window, &sidebar);
        // Selected by the arrow rather than by `activate`, which emits the
        // event straight from the entity and so never sets `selected`. Row 0 is
        // `a`: the comparator puts it before `b`.
        visual.simulate_keystrokes("down");
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
            Some(a)
        );

        visual.simulate_keystrokes("cmd-backspace");
        visual.run_until_parked();

        let (message, _) = cx.pending_prompt().expect("the delete must ask first");
        assert!(
            message.contains('a'),
            "the prompt names the note: {message:?}"
        );
        cx.simulate_prompt_answer("Move to Trash");
        visual.run_until_parked();

        assert_eq!(fs.read_to_string(Path::new("/vault/a.md")), None);
        assert_eq!(
            fs.trashed(Path::new("/vault/a.md")).as_deref(),
            Some("alpha")
        );
        let _ = notes;
    }

    /// Cancelling must leave everything exactly as it was. Without this the
    /// prompt could be deleted entirely and the test above would still pass.
    #[gpui::test]
    fn declining_the_prompt_keeps_the_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
        activate(&sidebar, cx, a);
        cx.run_until_parked();

        let mut visual = tree_focused(cx, window, &sidebar);
        visual.simulate_keystrokes("down");
        visual.simulate_keystrokes("cmd-backspace");
        visual.run_until_parked();
        cx.simulate_prompt_answer("Cancel");
        visual.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("alpha")
        );
    }

    /// Deleting the note on screen has to take the editor with it. A buffer
    /// whose file is gone is still typable, and every keystroke into it would
    /// be lost — the defect the unnamed launch buffer had.
    #[gpui::test]
    fn trashing_the_open_note_empties_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
        activate(&sidebar, cx, a);
        cx.run_until_parked();
        assert!(
            window
                .update(cx, |shell, _, _| shell.editor.is_some())
                .expect("the shell is open"),
            "the note is on screen to begin with"
        );

        notes.update(cx, |notes, cx| notes.trash(a, cx)).detach();
        cx.run_until_parked();

        assert!(
            window
                .update(cx, |shell, _, _| shell.editor.is_none())
                .expect("the shell is open"),
            "the editor must go with the note"
        );
        let _ = sidebar;
    }

    /// Deleting a note that is *not* on screen must leave the editor alone.
    ///
    /// Reachable with only this slice's keys: click `a` to open it, press
    /// `down` to select `b` without opening it, then delete. `NoteClosed(b)`
    /// fires while the editor is showing `a`.
    #[gpui::test]
    fn trashing_a_note_that_is_not_on_screen_leaves_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, b) = two_notes(cx, &fs);
        activate(&sidebar, cx, a);
        cx.run_until_parked();

        notes.update(cx, |notes, cx| notes.trash(b, cx)).detach();
        cx.run_until_parked();

        assert!(
            window
                .update(cx, |shell, _, _| shell.editor.is_some())
                .expect("the shell is open"),
            "the editor was showing a different note"
        );
        assert_eq!(fs.read_to_string(Path::new("/vault/b.md")), None);
    }

    /// After a delete the selection lands on a real row, so the next
    /// `cmd-backspace` has a target and does not act on a note that is gone.
    #[gpui::test]
    fn the_selection_moves_off_a_deleted_row(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, _notes, a, b) = two_notes(cx, &fs);

        let mut visual = tree_focused(cx, window, &sidebar);
        visual.simulate_keystrokes("down");
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
            Some(a)
        );

        visual.simulate_keystrokes("cmd-backspace");
        visual.run_until_parked();
        cx.simulate_prompt_answer("Move to Trash");
        visual.run_until_parked();

        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.selected()),
            Some(b),
            "the selection follows to the next row"
        );
    }

    /// The one regression this file exists to guard: the existing
    /// `simulate_keystrokes` tests in `editor/tests.rs` open a window with
    /// `EditorView` as the direct root, so they never exercise `AppShell` at
    /// all. Only this test opens the window the way `main.rs` actually does
    /// — through `AppShell` — and proves a keystroke still reaches the
    /// editor once it is no longer the window root.
    #[gpui::test]
    fn a_keystroke_through_app_shell_reaches_the_editor(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
        let notes = cx.update(|cx| cx.new(|cx| NoteStore::new(Arc::new(MemFs::new()), None, cx)));
        let window = cx.add_window(move |_, cx| AppShell::new(Some(editor), notes, cx));
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_input("hi");

        window
            .update(&mut visual, |_, _, cx| {
                assert_eq!(buffer.read(cx).text(), "hi");
            })
            .unwrap();
    }

    /// The one end-to-end test of the seam: a real click, through the
    /// subscription, into the editor. Nothing in `sidebar.rs` catches a
    /// subscription wired to the wrong entity, or left unbound.
    #[gpui::test]
    fn clicking_a_row_hands_its_buffer_to_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");

        let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        // Vertical middle of the first row, past its `pl`.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();

        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }

    /// A `current` the editor disagrees with saves *nothing*: the note it picks
    /// is clean, so `⌘S` returns early and the dirty note stays dirty, silently.
    /// Deterministic under the randomizing test scheduler
    /// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:641-643`)
    /// because only one load is in flight and the gate decides when it lands.
    #[gpui::test]
    fn an_abandoned_load_does_not_supersede_a_later_activation(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

        // Opened up front so the activation below takes the already-open path.
        activate(&sidebar, cx, a);
        cx.run_until_parked();

        let gate = fs.block_load("/vault/b.md");
        activate(&sidebar, cx, b);
        cx.run_until_parked();
        activate(&sidebar, cx, a);
        cx.run_until_parked();

        gate.release();
        cx.run_until_parked();

        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(
                editor.buffer().read(cx).text(),
                "alpha",
                "the last note activated is the note the editor ends on"
            );
        });
        notes.update(cx, |notes, _| {
            assert_eq!(
                notes.current(),
                Some(a),
                "`save_current` reads this; pointing it at the abandoned note \
                 makes `cmd-s` a silent no-op"
            );
        });
    }

    /// The gates release in the opposite order to the activations: against the
    /// unfixed code both loads land and the last wins. Against the fixed code
    /// activating `b` tears `a`'s whole chain down, so the order stops mattering.
    #[gpui::test]
    fn the_current_note_is_the_one_in_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

        let gate_a = fs.block_load("/vault/a.md");
        let gate_b = fs.block_load("/vault/b.md");

        activate(&sidebar, cx, a);
        cx.run_until_parked();
        activate(&sidebar, cx, b);
        cx.run_until_parked();

        gate_b.release();
        cx.run_until_parked();
        gate_a.release();
        cx.run_until_parked();

        let current = notes
            .update(cx, |notes, _| notes.current())
            .expect("a note is current");
        assert_eq!(current, b, "the last activation wins");

        let in_store = notes.update(cx, |notes, _| {
            notes.buffer_for(current).map(Entity::entity_id)
        });
        let in_editor = editor(&window, cx).update(cx, |editor, _| editor.buffer().entity_id());
        assert_eq!(
            in_store,
            Some(in_editor),
            "the current note and the editor's buffer are the same entity"
        );
    }

    /// Even with no load to cancel, the rebind is one turn late; `⌘S` in that
    /// turn silently writes nothing. Hence `set_current` after the bind.
    ///
    /// The emit hop costs no turn: the subscriber runs when the outermost
    /// update flushes, so `activate` returns with the task spawned but unpolled.
    #[gpui::test]
    fn an_activation_records_the_note_only_once_the_editor_holds_it(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (window, sidebar, notes, a, b) = two_notes(cx, &fs);

        // Both opened, so the third activation takes the already-open path.
        activate(&sidebar, cx, a);
        cx.run_until_parked();
        activate(&sidebar, cx, b);
        cx.run_until_parked();

        // Deliberately not parked: this is the turn `⌘S` can land in.
        activate(&sidebar, cx, a);

        assert_eq!(
            notes.update(cx, |notes, _| notes.current()),
            Some(b),
            "the editor still holds `b`, so `b` is still the note a save writes"
        );
        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "beta");
        });

        cx.run_until_parked();
        assert_eq!(notes.update(cx, |notes, _| notes.current()), Some(a));
        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }

    /// The whole reason this change is not four lines of `div`. The editor does
    /// not exist when the window opens, so something has to move focus to it
    /// when it appears. Nothing else can: `EditorView` takes keystrokes through
    /// `track_focus` (`editor/mod.rs:259`), so an editor that is built and
    /// rendered but never focused swallows every one of them.
    #[gpui::test]
    fn typing_reaches_a_note_opened_after_launch(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "");
        let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let a = entry_id(&notes, cx, "a");

        activate(&sidebar, cx, a);
        cx.run_until_parked();

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_input("hi");
        visual.run_until_parked();

        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(
                editor.buffer().read(cx).text(),
                "hi",
                "the editor created by the activation never received focus"
            );
        });
    }

    /// `cmd-s` is bound at the `AppShell` context, and with no note open the
    /// window's focus is on `AppShell`'s own handle. Without `track_focus` on
    /// the root div that handle is in no dispatch node, so dispatch falls back
    /// to the view boundary *above* the div carrying `.on_action`
    /// (`window.rs:6244-6251`, `key_dispatch.rs:611-614`, `view.rs:462-476`)
    /// and the action never arrives.
    ///
    /// Asserting reachability rather than an effect is deliberate. "No panic and
    /// no write" is true either way: `save_current` returns at its `None` guard
    /// whether or not it was ever called, so the two arms are indistinguishable
    /// by outcome. `is_action_available` (`window.rs:3187-3193`) asks the
    /// dispatch tree directly instead. It reads the *rendered* frame, which
    /// exists here because `open_window` draws once before returning
    /// (`app.rs:1363-1368`) for exactly this reason.
    #[gpui::test]
    fn the_save_action_reaches_the_shell_with_no_note_open(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();
        cx.run_until_parked();

        let reachable = window
            .update(cx, |_, window, cx| window.is_action_available(&Save, cx))
            .unwrap();
        assert!(
            reachable,
            "`cmd-s` cannot reach `AppShell::save` from the empty state"
        );

        // And it does not panic on the way through.
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_keystrokes("cmd-s");
        visual.run_until_parked();
    }

    /// The transition, in one test: the empty state is what an activation
    /// replaces, and it replaces it with the note that was activated.
    #[gpui::test]
    fn activating_a_note_replaces_the_empty_state(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
        let a = entry_id(&notes, cx, "a");
        assert!(
            window
                .update(cx, |shell, _, _| shell.editor.is_none())
                .unwrap()
        );

        activate(&sidebar, cx, a);
        cx.run_until_parked();

        editor(&window, cx).update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }

    /// The hole this change closes, and the only test that guards it.
    ///
    /// A launch with no argument, or on a vault, used to build a real editable
    /// `Buffer` that `NoteStore` never tracked: unreachable by `⌘S`, by the
    /// autosave timer and by the quit flush alike, so everything typed into it
    /// was lost. Restoring that unconditional build is the regression that
    /// undoes the change, and every other test in this file builds `AppShell`
    /// by hand — passing its own `None`, which no production decision can flip.
    /// Only this test calls `root`, so only this test would notice.
    ///
    /// It asserts the absence of an editor rather than the presence of the
    /// empty state: `check.sh` renders nothing, so "the text was drawn" is not
    /// a claim a `#[gpui::test]` can make. The `empty-no-vault` shot is that
    /// check.
    #[gpui::test]
    fn root_builds_an_editor_only_for_a_named_file(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        let scanned =
            futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault"))).expect("scan");
        let loaded = futures::executor::block_on(fs.load(Path::new("/vault/a.md"))).expect("load");

        for (launched, expected) in [
            (Launch::Nothing, false),
            (Launch::Vault(scanned), false),
            (
                Launch::Note(
                    PathBuf::from("/vault/a.md"),
                    loaded.text.clone(),
                    loaded.stamp,
                ),
                true,
            ),
        ] {
            let fs = Arc::clone(&fs) as Arc<dyn Fs>;
            // `App::open_window`, not `cx.add_window`: the test helpers all
            // build the root view themselves, and `root` hands back an already
            // built `Entity<AppShell>` — which is the shape `open_window`'s
            // builder takes and theirs do not.
            let window = cx
                .update(|cx| {
                    let bounds = Bounds::maximized(None, cx);
                    cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            ..Default::default()
                        },
                        |_, cx| root(fs, launched, cx),
                    )
                })
                .expect("open the window");
            let has_editor = window
                .update(cx, |shell, _, _| shell.editor.is_some())
                .unwrap();
            assert_eq!(has_editor, expected);
        }
    }
}
