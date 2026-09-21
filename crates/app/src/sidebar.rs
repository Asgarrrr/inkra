//! The vault tree: a linear walk over path-ordered entries, never a per-frame
//! tree rebuild. `Vault::entries` yields a directory's descendants in a
//! contiguous run after it, so collapsing one is a forward skip.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    App, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    Styled, Task, Window, div, px, rgb, uniform_list,
};
use inkra_vault::{EntryId, Kind};

use crate::editor::EditorView;
use crate::vault::NoteStore;

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    /// Held for `EditorView::open_buffer`, the only path that may rebind.
    editor: Entity<EditorView>,
    /// Expansion is UI state, never vault state.
    expanded: HashSet<EntryId>,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
    /// Held, never detached: dropping a `Task` cancels it
    /// (`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`), so a
    /// superseded load cannot rebind the editor behind a later click. The
    /// opposite of `AppShell::save`, deliberately.
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

    /// `RelPath::starts_with` is component-wise, so the skip past a collapsed
    /// directory never swallows the sibling `Projects.md`.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.visible.clear();
        let Some(vault) = self.notes.read(cx).vault().cloned() else {
            // Unreachable today: the vault is fixed at construction.
            cx.notify();
            return;
        };
        // Collected, not iterated: the walk borrows `cx`, which `notify` needs.
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

    /// Bind and `set_current` run with no `await` between them, so nothing can
    /// observe one without the other.
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

    /// Split out only so a test can read the dot: nothing gets text back out
    /// of the `Div`s `render_rows` returns.
    fn row_label(&self, row: &Row, cx: &App) -> String {
        let dot = if self.notes.read(cx).is_modified(row.id, cx) {
            " •"
        } else {
            ""
        };
        format!("{}{dot}", row.name)
    }

    /// `cx.processor` re-enters the sidebar entity here
    /// (`gpui/src/app/context.rs:260-267`); that is the only reason
    /// `cx.listener` is reachable from `uniform_list`'s `&mut App` closure.
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
                // Reading inside the draw is what repaints the dot; no
                // `cx.observe` needed and `Entity::cached` would break it
                // (`app.rs:2789-2823`, `view.rs:488`).
                let label = self.row_label(&row, cx);
                // `Vault::scan` stops at depth 64, so nothing saturates.
                let depth = f32::from(u8::try_from(row.depth).unwrap_or(u8::MAX));
                div()
                    .flex()
                    .flex_row()
                    // Load-bearing: an `auto` width is clickable only where the
                    // text reaches (`elements/uniform_list.rs:508-510`).
                    .w_full()
                    .h(px(22.))
                    .pl(px(8. + 12. * depth))
                    // Load-bearing: row height is measured from item 0 alone
                    // (`elements/uniform_list.rs:658-680`), so a wrapping
                    // filename overflows its row.
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
        // Load-bearing: the default `ListSizingBehavior::Auto` resolves to zero
        // height and paints nothing (`elements/list.rs:192-193`).
        .size_full()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Instant;

    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, prelude::*, px};
    use inkra_text::{Buffer, ByteOffset};
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{Kind, Row, Sidebar};
    use crate::editor::EditorView;
    use crate::vault::NoteStore;

    /// Stops short of the sidebar: `add_window` constructs the view itself.
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

    fn open_with_store(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (Entity<Sidebar>, Entity<NoteStore>, Entity<EditorView>) {
        let (notes, editor) = parts(cx, fs);
        let sidebar = cx.update(|cx| cx.new(|cx| Sidebar::new(notes.clone(), editor.clone(), cx)));
        (sidebar, notes, editor)
    }

    /// Trailing `/` on directories: the display name is the file stem, so
    /// `Projects` and `Projects.md` are otherwise indistinguishable.
    fn labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
        sidebar.update(cx, |sidebar, _| sidebar.visible.iter().map(label).collect())
    }

    fn label(row: &Row) -> String {
        match row.kind {
            Kind::Dir => format!("{}/", row.name),
            Kind::Note => row.name.clone(),
        }
    }

    /// Dot included, unlike `labels`, which rebuilds the name itself.
    fn row_labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
        sidebar.update(cx, |sidebar, cx| {
            let rows = sidebar.visible.clone();
            rows.iter().map(|row| sidebar.row_label(row, cx)).collect()
        })
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

    /// A byte-prefix skip would swallow `Projects.md` with the folder.
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

    /// Opening `Projects` must not open `Projects/deep` with it.
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

    /// Nothing in the type system stops every row's `on_mouse_down` seeing the
    /// same press (`elements/uniform_list.rs:508-510`).
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

    /// A real mouse event, not a call to `activate`: only the layout decides
    /// whether a click lands. Discriminates `.w_full()` — headless text is
    /// measured at 0.6 × `font_size` per glyph
    /// (`vendor/zed-reference/crates/gpui/src/platform.rs:1288-1296`), so an
    /// `auto` row around the stem `a` ends at 17.6px and x=20 misses.
    ///
    /// Does *not* cover `.size_full()`: a root's `auto` size is stretched to
    /// fill (`window.rs:3542-3545`), unlike the real parent in `shell.rs`.
    #[gpui::test]
    fn clicking_a_row_hands_its_buffer_to_the_editor(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");

        let (notes, editor) = parts(cx, fs as Arc<dyn Fs>);
        let for_window = editor.clone();
        let window = cx.add_window(move |_, cx| Sidebar::new(notes, for_window, cx));
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        // Vertical middle of the first row, past its `pl`.
        visual.simulate_click(point(px(20.), px(11.)), Modifiers::none());
        visual.run_until_parked();

        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer().read(cx).text(), "alpha");
        });
    }

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

    /// A `current` the editor disagrees with saves *nothing*: the note it picks
    /// is clean, so `⌘S` returns early and the dirty note stays dirty, silently.
    /// Deterministic under the randomizing test scheduler
    /// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:641-643`)
    /// because only one load is in flight and the gate decides when it lands.
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

    /// The gates release in the opposite order to the clicks: against the
    /// unfixed code both loads land and the last wins. Against the fixed code
    /// clicking `b` tears `a`'s whole chain down, so the order stops mattering.
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

    /// Even with no load to cancel, the rebind is one turn late; `⌘S` in that
    /// turn silently writes nothing. Hence `set_current` after the bind.
    #[gpui::test]
    fn a_click_records_the_note_only_once_the_editor_holds_it(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (sidebar, notes, editor, a, b) = two_notes(cx, &fs);

        // Both opened, so the third click takes the already-open path.
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&a, cx));
        cx.run_until_parked();
        sidebar.update(cx, |sidebar, cx| sidebar.activate(&b, cx));
        cx.run_until_parked();

        // Deliberately not parked: this is the turn `⌘S` can land in.
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

    /// Asserts the label, not `is_modified`: the predicate passes with the dot
    /// deleted. One note edited, so a marker on every row also fails.
    #[gpui::test]
    fn an_edited_note_is_marked_in_the_tree_until_it_is_saved(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        let (sidebar, notes, editor, _a, b) = two_notes(cx, &fs);

        sidebar.update(cx, |sidebar, cx| sidebar.activate(&b, cx));
        cx.run_until_parked();
        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "opening a note changes nothing about it"
        );

        let buffer = editor.update(cx, |editor, _| editor.buffer().clone());
        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        });

        assert_eq!(row_labels(&sidebar, cx), ["a", "b •"]);

        notes.update(cx, NoteStore::save_current).detach();
        cx.run_until_parked();

        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "the write caught the file up, so the note is no longer modified"
        );
    }
}
