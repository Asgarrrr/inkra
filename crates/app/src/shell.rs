//! The window's root view: composes the sidebar and the editor.
//!
//! Named, opaque slots rather than an ad hoc `div` tree assembled at render
//! time — see `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`
//! for why, and what this deliberately leaves out (sidebar content, resize,
//! a status bar, multi-pane).

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Task, Window, div, px, rgb,
};
use inkra_vault::EntryId;

use crate::editor::EditorView;
use crate::sidebar::{NoteActivated, Sidebar};
use crate::vault::{NoteStore, Save, SaveFailed};

pub(crate) struct AppShell {
    editor: Entity<EditorView>,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
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
    pub(crate) fn new(
        editor: Entity<EditorView>,
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
        Self {
            editor,
            notes,
            sidebar,
            activation: None,
        }
    }

    /// Bind and `set_current` run with no `await` between them, so nothing can
    /// observe one without the other. A failed load is swallowed: the spec's
    /// deferred status line would surface it here.
    fn open_note(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let editor = self.editor.clone();
        let notes = self.notes.clone();
        let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
        self.activation = Some(cx.spawn(async move |_, cx| {
            let Some(buffer) = opened.await else { return };
            editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx));
            notes.update(cx, |notes, cx| notes.set_current(id, cx));
        }));
    }

    /// Nothing to hold: the write chain lives on the note, in the store, which
    /// outlives every save. `⌘S` only preempts the timer that chain is already
    /// waiting on.
    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.notes.update(cx, NoteStore::save_current);
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for AppShell {
    // Matches EditorView::render's own allow (editor/mod.rs): RRGGBB hex
    // literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            // Scoped here rather than on `EditorView`: the sidebar will be
            // focusable, and a binding scoped to the editor would do nothing
            // while a note row holds focus. The action still reaches this
            // handler from the editor, because dispatch walks from the focused
            // node up to the root.
            .key_context("AppShell")
            .on_action(cx.listener(Self::save))
            .flex()
            .flex_row()
            .size_full()
            .child(
                // flex_none (grow 0, shrink 0): never competes with the
                // editor for space.
                div()
                    .flex_none()
                    .w(px(180.))
                    .h_full()
                    .bg(rgb(0x181818))
                    .child(self.sidebar.clone()),
            )
            .child(
                // flex_1 (grow 1, shrink 1, basis 0%) makes this wrapper
                // claim all remaining row width — gpui/styled.rs:181-186.
                // EditorView's own .size_full() then fills 100% of this
                // wrapper's resolved size; EditorView never needs to know
                // it's no longer the window root.
                div().flex_1().h_full().child(self.editor.clone()),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use gpui::{
        Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowHandle, point,
        prelude::*, px,
    };
    use inkra_text::Buffer;
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{AppShell, EditorView, EntryId, NoteActivated, NoteStore, Sidebar};
    use crate::keymap::key_bindings;

    /// Rooted at `AppShell`, not at the `Sidebar`: that puts the sidebar under
    /// its real 180px parent, and it is the only way to reach the subscription
    /// `AppShell::new` sets up.
    fn open_vault(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (
        WindowHandle<AppShell>,
        Entity<Sidebar>,
        Entity<NoteStore>,
        Entity<EditorView>,
    ) {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        let vault = cx.update(|cx| cx.new(|_| scanned));
        let notes = cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer, cx)));
        let window = cx.add_window({
            let (editor, notes) = (editor.clone(), notes.clone());
            move |_, cx| AppShell::new(editor, notes, cx)
        });
        let sidebar = window
            .update(cx, |shell, _, _| shell.sidebar.clone())
            .expect("the shell is open");
        (window, sidebar, notes, editor)
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

    fn two_notes(
        cx: &mut TestAppContext,
        fs: &Arc<MemFs>,
    ) -> (
        Entity<Sidebar>,
        Entity<NoteStore>,
        Entity<EditorView>,
        EntryId,
        EntryId,
    ) {
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (_window, sidebar, notes, editor) = open_vault(cx, Arc::clone(fs) as Arc<dyn Fs>);
        let a = entry_id(&notes, cx, "a");
        let b = entry_id(&notes, cx, "b");
        (sidebar, notes, editor, a, b)
    }

    /// Indistinguishable from a click to `AppShell`, and it needs no `Row`, so
    /// nothing in `sidebar.rs` widens for it.
    fn activate(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, id: EntryId) {
        sidebar.update(cx, |_, cx| cx.emit(NoteActivated(id)));
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
        let window = cx.add_window(move |_, cx| AppShell::new(editor, notes, cx));
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

        let (window, _sidebar, _notes, editor) = open_vault(cx, fs as Arc<dyn Fs>);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        // Vertical middle of the first row, past its `pl`.
        visual.simulate_click(point(px(20.), px(11.)), Modifiers::none());
        visual.run_until_parked();

        editor.update(cx, |editor, cx| {
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
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

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

        editor.update(cx, |editor, cx| {
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
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

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
        let in_editor = editor.update(cx, |editor, _| editor.buffer().entity_id());
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
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

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
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "beta");
        });

        cx.run_until_parked();
        assert_eq!(notes.update(cx, |notes, _| notes.current()), Some(a));
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }
}
