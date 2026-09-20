//! The vault tree.
//!
//! Rendered as a linear walk over path-ordered entries, never as a per-frame
//! tree rebuild: `inkra_vault`'s comparator guarantees a directory's
//! descendants are contiguous, so collapsing one is a forward skip.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    Context, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    Styled, Task, Window, div, px, rgb, uniform_list,
};
use inkra_vault::{EntryId, Kind};

use crate::editor::EditorView;
use crate::vault::NoteStore;

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    /// Held so a clicked note can reach `EditorView::open_buffer`, the only
    /// path that may rebind the view's buffer.
    editor: Entity<EditorView>,
    /// Expansion is UI state, never vault state.
    expanded: HashSet<EntryId>,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
    /// The click currently being served. Held, never detached: a new click
    /// replaces it, and dropping a `Task` cancels it
    /// (`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`), so a
    /// superseded load can never rebind the editor behind a later click.
    ///
    /// The opposite of `AppShell::save`, which detaches deliberately — see the
    /// comment there. A save has no successor to be superseded by, and
    /// dropping its task would cancel the write it just started.
    activation: Option<Task<()>>,
}

#[derive(Clone)]
struct Row {
    id: EntryId,
    depth: usize,
    kind: Kind,
    name: String,
}

impl Sidebar {
    pub(crate) fn new(
        notes: Entity<NoteStore>,
        editor: Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut sidebar = Self {
            notes,
            editor,
            expanded: HashSet::new(),
            visible: Vec::new(),
            activation: None,
        };
        sidebar.rebuild(cx);
        sidebar
    }

    /// One forward pass. A collapsed directory's descendants are skipped by
    /// advancing while `RelPath::starts_with` holds — component-wise, so the
    /// sibling `Projects.md` is never swallowed by the folder `Projects`.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            return;
        };
        // Collected rather than iterated in place: the walk borrows `cx`, and
        // `cx.notify()` below needs it back.
        let entries: Vec<_> = vault.read(cx).entries().cloned().collect();

        let mut i = 0;
        while i < entries.len() {
            let entry = &entries[i];
            self.visible.push(Row {
                id: entry.id,
                depth: entry.path.depth(),
                kind: entry.kind,
                name: entry.path.display_name(),
            });
            i += 1;
            if entry.kind == Kind::Dir && !self.expanded.contains(&entry.id) {
                while i < entries.len() && entries[i].path.starts_with(&entry.path) {
                    i += 1;
                }
            }
        }
        cx.notify();
    }

    fn toggle(&mut self, id: EntryId, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild(cx);
    }

    /// A directory toggles; a note loads and is handed to
    /// `EditorView::open_buffer`.
    ///
    /// Binding the buffer and recording the note as current happen together,
    /// with no `await` between them, so nothing can observe one without the
    /// other. Assigning the task drops whatever click it supersedes, which
    /// cancels that load before it can rebind anything.
    fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        let editor = self.editor.clone();
        let notes = self.notes.clone();
        let id = row.id;
        let opened = self
            .notes
            .update(cx, |notes, cx| notes.open_note(row.id, cx));
        self.activation = Some(cx.spawn(async move |_, cx| {
            let Some(buffer) = opened.await else { return };
            editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx));
            notes.update(cx, |notes, cx| notes.set_current(id, cx));
        }));
    }

    /// The rows `uniform_list` asked for.
    ///
    /// Reached through `cx.processor`, which holds the sidebar entity and
    /// re-enters it here (`gpui/src/app/context.rs:260-267`). That is what
    /// makes `cx.listener` available at all: the closure `uniform_list` takes
    /// is `'static + Fn(Range<usize>, &mut Window, &mut App)`, and an `App` has
    /// no view to listen on.
    #[allow(clippy::unreadable_literal)]
    fn render_rows(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        range
            .filter_map(|ix| self.visible.get(ix).cloned())
            .map(|row| {
                // Read, never observed. gpui records every entity a window
                // touched while drawing (`app/entity_map.rs:213-214`, recorded
                // by `window.rs:3421-3433`), and `App::notify` dirties any
                // window that read the notifying entity
                // (`app.rs:2789-2823`). Both the store and the row's buffer are
                // read right here, so an edit or a save redraws this window;
                // `.child(entity)` is uncached (`view.rs:100-104`), so the
                // sidebar re-renders with it and the dot follows. A `cx.observe`
                // would add nothing. Wrapping the sidebar in `Entity::cached`
                // would break it — that is the only way to reach the
                // `dirty_views` check at `view.rs:483-498`.
                let dot = if self.notes.read(cx).is_modified(row.id, cx) {
                    " •"
                } else {
                    ""
                };
                let label = format!("{}{dot}", row.name);
                // Saturating rather than cast: `Vault::scan` stops at depth 64,
                // so nothing here is lost, and a lossy `as f32` is denied.
                let depth = f32::from(u8::try_from(row.depth).unwrap_or(u8::MAX));
                div()
                    .flex()
                    .flex_row()
                    // Required, not cosmetic: `uniform_list` lays each item out
                    // as its own root (`elements/uniform_list.rs:508-510`), so
                    // an `auto` width shrinks to the label and the row is only
                    // clickable where its text happens to reach. Everything
                    // right of a short filename would be dead space.
                    .w_full()
                    .h(px(22.))
                    .pl(px(8. + 12. * depth))
                    // Required, not cosmetic: item height is measured from item
                    // 0 alone, at MaxContent width
                    // (`gpui/elements/uniform_list.rs:658-680`), so a long
                    // filename measures unwrapped and then wraps at paint,
                    // overflowing the row it was measured for.
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(0xd0d0d0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |sidebar, _event, _window, cx| {
                            sidebar.activate(&row, cx);
                        }),
                    )
                    .child(label)
            })
            .collect()
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        uniform_list(
            "vault-tree",
            self.visible.len(),
            cx.processor(Self::render_rows),
        )
        // Required: `ListSizingBehavior` defaults to `Auto`
        // (`gpui/elements/list.rs:190-192`), whose branch requests layout
        // with no children (`uniform_list.rs:315-318`). Without this the
        // list resolves to zero height inside the sidebar's fixed-width div
        // and paints nothing.
        .size_full()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, prelude::*, px};
    use inkra_text::Buffer;
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{Kind, Row, Sidebar};
    use crate::editor::EditorView;
    use crate::vault::NoteStore;

    /// A scanned vault behind a store, plus the editor it opens notes into.
    ///
    /// Stops short of the sidebar so a test can decide whether to build one
    /// standalone or as a window root — `add_window` constructs the view
    /// itself and will not adopt one made earlier.
    fn parts(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> (Entity<NoteStore>, Entity<EditorView>) {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        let vault = cx.update(|cx| cx.new(|_| scanned));
        let notes = cx.update(|cx| cx.new(|_| NoteStore::new(fs, Some(vault))));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer, cx)));
        (notes, editor)
    }

    /// The same, with a sidebar that is never drawn.
    fn open(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> (Entity<Sidebar>, Entity<EditorView>) {
        let (sidebar, _notes, editor) = open_with_store(cx, fs);
        (sidebar, editor)
    }

    /// The same again, keeping the store: the tests about which note is
    /// *current* have to read it.
    fn open_with_store(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (Entity<Sidebar>, Entity<NoteStore>, Entity<EditorView>) {
        let (notes, editor) = parts(cx, fs);
        let sidebar = cx.update(|cx| cx.new(|cx| Sidebar::new(notes.clone(), editor.clone(), cx)));
        (sidebar, notes, editor)
    }

    /// Every visible row, top to bottom, with a trailing `/` on directories.
    ///
    /// The marker is what makes the assertions readable at all: `RelPath`'s
    /// display name is the file stem, so the folder `Projects` and the folder
    /// note `Projects.md` are both labelled `Projects` in the real sidebar.
    fn labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
        sidebar.update(cx, |sidebar, _| sidebar.visible.iter().map(label).collect())
    }

    fn label(row: &Row) -> String {
        match row.kind {
            Kind::Dir => format!("{}/", row.name),
            Kind::Note => row.name.clone(),
        }
    }

    fn row(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, wanted: &str) -> Row {
        sidebar.update(cx, |sidebar, _| {
            sidebar
                .visible
                .iter()
                .find(|row| label(row) == wanted)
                .cloned()
                .expect("the row is visible")
        })
    }

    /// The forward skip is component-wise, so the folder note `Projects.md`
    /// must survive collapsing the folder `Projects` — the trap a byte-prefix
    /// skip would fall into.
    #[gpui::test]
    fn collapsing_a_folder_hides_its_children_and_nothing_else(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "p");
        fs.insert_file("/vault/Projects.md", "n");
        fs.insert_file("/vault/a.md", "a");

        let (sidebar, _editor) = open(cx, fs as Arc<dyn Fs>);

        // Folders start collapsed.
        assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);

        let projects = row(&sidebar, cx, "Projects/");
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));
        assert_eq!(labels(&sidebar, cx), ["Projects/", "plan", "Projects", "a"]);

        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));
        assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);
    }

    /// Expanding a folder reveals its children, not its grandchildren.
    ///
    /// Each nested folder keeps its own expansion state, so opening `Projects`
    /// must not open `Projects/deep` with it. Found by eye in the render pass:
    /// the one-level test above cannot see this.
    #[gpui::test]
    fn expanding_a_folder_leaves_its_nested_folders_collapsed(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/deep/x.md", "x");
        fs.insert_file("/vault/Projects/plan.md", "p");

        let (sidebar, _editor) = open(cx, fs as Arc<dyn Fs>);
        assert_eq!(labels(&sidebar, cx), ["Projects/"]);

        let projects = row(&sidebar, cx, "Projects/");
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&projects, cx));

        assert_eq!(
            labels(&sidebar, cx),
            ["Projects/", "deep/", "plan"],
            "`x` lives inside `deep`, which nobody expanded"
        );
    }

    /// One click activates one row.
    ///
    /// `uniform_list` lays every item out as its own root
    /// (`gpui/elements/uniform_list.rs:508-510`), so nothing in the type system
    /// stops each row's `on_mouse_down` from seeing the same press. With four
    /// rows, a press on the first must toggle the folder and leave the notes
    /// shut — a list that dispatched to all of them would end up showing the
    /// last note instead.
    #[gpui::test]
    fn a_press_on_one_row_does_not_reach_the_others(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "plan");
        fs.insert_file("/vault/Projects.md", "folder note");
        fs.insert_file("/vault/a.md", "alpha");

        let (notes, editor) = parts(cx, fs as Arc<dyn Fs>);
        let for_window = editor.clone();
        let window = cx.add_window(move |_, cx| Sidebar::new(notes, for_window, cx));
        let mut visual = VisualTestContext::from_window(window.into(), cx);

        // The first row is the folder `Projects`, at rows of `h(px(22.))`.
        visual.simulate_click(point(px(20.), px(11.)), Modifiers::none());
        visual.run_until_parked();

        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.buffer().read(cx).text(),
                "",
                "pressing a folder must open no note at all"
            );
        });
        let sidebar = window.root(cx).expect("root view");
        assert_eq!(
            labels(&sidebar, cx),
            ["Projects/", "plan", "Projects", "a"],
            "exactly one row responded"
        );
    }

    /// Clicking a note is a load followed by `EditorView::open_buffer`.
    ///
    /// Driven through a real mouse event on a drawn window rather than by
    /// calling `activate`, because it is the layout that decides whether a
    /// click can land at all, and no test that calls `activate` directly can
    /// see that. Discriminating for `.w_full()` on the row: without it the row
    /// shrinks to its label — 8px here, since text measures to nothing in a
    /// headless test — and the click at x=20 misses.
    ///
    /// Not discriminating for `.size_full()` on the list. The sidebar is this
    /// window's root, and a root's `auto` size is stretched to fill
    /// (`window.rs:3542-3545`), which is exactly what the sidebar's fixed-width
    /// parent in `shell.rs` does not do. That one is still an eyeball check.
    #[gpui::test]
    fn clicking_a_row_hands_its_buffer_to_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");

        let (notes, editor) = parts(cx, fs as Arc<dyn Fs>);
        let for_window = editor.clone();
        let window = cx.add_window(move |_, cx| Sidebar::new(notes, for_window, cx));
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        // The vertical middle of the first row, past its `pl`. Rows are
        // `h(px(22.))` and the list starts at the window origin.
        visual.simulate_click(point(px(20.), px(11.)), Modifiers::none());
        // The load runs on a background task.
        visual.run_until_parked();

        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }

    /// A vault of two notes, a sidebar over it, and both rows resolved by name.
    fn two_notes(
        cx: &mut TestAppContext,
        fs: &Arc<MemFs>,
    ) -> (
        Entity<Sidebar>,
        Entity<NoteStore>,
        Entity<EditorView>,
        Row,
        Row,
    ) {
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");
        let (sidebar, notes, editor) = open_with_store(cx, Arc::clone(fs) as Arc<dyn Fs>);
        let a = row(&sidebar, cx, "a");
        let b = row(&sidebar, cx, "b");
        (sidebar, notes, editor, a, b)
    }

    /// An abandoned load must not land on top of a later, deliberate click.
    ///
    /// Click `b`, change your mind while it is still loading, click `a` —
    /// already open, so it resolves without touching the filesystem at all.
    /// When `b`'s load finally lands, nothing may rebind the editor to it and
    /// nothing may record it as current. `save_current` reads `current`, so a
    /// `current` the editor disagrees with does not save the wrong file: it
    /// saves *nothing*, because the note it picks is clean and the version
    /// comparison returns early. The note on screen stays dirty with no error.
    ///
    /// Deterministic under the randomizing test scheduler
    /// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:641-643`):
    /// only one load is ever in flight, and the gate decides when it lands.
    #[gpui::test]
    fn an_abandoned_load_does_not_supersede_a_later_click(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

        // Opened up front so the click below takes the already-open path.
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));
        cx.run_until_parked();

        let gate = fs.block_load("/vault/b.md");
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&b, cx));
        cx.run_until_parked();
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));
        cx.run_until_parked();

        gate.release();
        cx.run_until_parked();

        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.buffer().read(cx).text(),
                "alpha",
                "the last note clicked is the note the editor ends on"
            );
        });
        notes.update(cx, |notes, _| {
            assert_eq!(
                notes.current(),
                Some(a.id),
                "`save_current` reads this; pointing it at the abandoned note \
                 makes `cmd-s` a silent no-op"
            );
        });
    }

    /// Two loads clicked in a row leave `current` naming the note the editor
    /// holds.
    ///
    /// Both notes are unopened, so both take the slow path, and both gates are
    /// held while the second click supersedes the first. Against the unfixed
    /// code both loads land and whichever lands last wins, which is why the
    /// gates release in the opposite order to the clicks. Against the fixed
    /// code there is no such interleaving to resolve: clicking `b` drops
    /// `activation`, which tears `a`'s whole load chain down — continuation,
    /// `open_note` task, and the `gate_a` wait inside it — during the
    /// `run_until_parked` below. `gate_a.release()` then has nothing left to
    /// release, and the test would pass with the two releases in either order.
    #[gpui::test]
    fn the_current_note_is_the_one_in_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

        let gate_a = fs.block_load("/vault/a.md");
        let gate_b = fs.block_load("/vault/b.md");

        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));
        cx.run_until_parked();
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&b, cx));
        cx.run_until_parked();

        gate_b.release();
        cx.run_until_parked();
        gate_a.release();
        cx.run_until_parked();

        let current = notes
            .update(cx, |notes, _| notes.current())
            .expect("a note is current");
        assert_eq!(current, b.id, "the last click wins");

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

    /// Clicking an already-open note must not record it as current before the
    /// editor holds it.
    ///
    /// Needs no second note in flight and no cancellation: the click resolves
    /// without a load, but the editor is still rebound one turn later, from the
    /// spawned task. `⌘S` in that window reads `current`, finds the freshly
    /// clicked note clean, and returns early — so the dirty note still on
    /// screen is never written and nothing says so.
    ///
    /// This is what makes `set_current` a *single writer placed after the
    /// bind*, rather than something `open_note` may do on its own schedule.
    #[gpui::test]
    fn a_click_records_the_note_only_once_the_editor_holds_it(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

        // Both notes get opened, so the third click takes the already-open
        // path — the one that resolves with no load at all.
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));
        cx.run_until_parked();
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&b, cx));
        cx.run_until_parked();

        // Deliberately not parked: this is the turn between the click and the
        // rebind, and `⌘S` can land in it.
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));

        assert_eq!(
            notes.update(cx, |notes, _| notes.current()),
            Some(b.id),
            "the editor still holds `b`, so `b` is still the note a save writes"
        );
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "beta");
        });

        cx.run_until_parked();
        assert_eq!(notes.update(cx, |notes, _| notes.current()), Some(a.id));
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }
}
