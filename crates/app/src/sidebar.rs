//! The vault tree: a linear walk over path-ordered entries, never a per-frame
//! tree rebuild. `Vault::entries` yields a directory's descendants in a
//! contiguous run after it, so collapsing one is a forward skip.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    App, Context, Div, Entity, EventEmitter, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Render, Styled, Subscription, Window, div, px, uniform_list,
};
use inkra_vault::{EntryId, Kind};

use crate::theme;
use crate::vault::NoteStore;

/// The left gutter every row starts from, and what each level of nesting adds.
const INDENT_BASE: Pixels = px(8.);
/// See [`INDENT_BASE`].
const INDENT_STEP: Pixels = px(12.);

/// Where the note goes is `AppShell`'s to decide, so any widget that can open
/// one — a tab bar, a backlinks panel — plugs in by emitting this, with no
/// change here. A struct, not a one-variant enum: `EventEmitter<E>` is generic
/// over the event type (`gpui/src/gpui.rs:296`), so a second signal is a
/// second type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteActivated(pub(crate) EntryId);

impl EventEmitter<NoteActivated> for Sidebar {}

pub(crate) struct Sidebar {
    notes: Entity<NoteStore>,
    /// Expansion is UI state, never vault state.
    expanded: HashSet<EntryId>,
    /// Derived when the vault or `expanded` changes, never per frame.
    visible: Vec<Row>,
    /// Rebuilds `visible` when the vault's contents change. Never read; holding
    /// it is the point, because dropping a `Subscription` cancels it.
    ///
    /// Without this the cache is rebuilt only by `new` and `toggle`, so a file
    /// that appears in the vault while the window is open — the conflict file a
    /// clobbered write preserves is the only one today — never reaches a row.
    /// The vault's own `notify` had no subscriber at all.
    _vault_changed: Option<Subscription>,
}

#[derive(Clone)]
struct Row {
    id: EntryId,
    depth: usize,
    kind: Kind,
    name: String,
}

impl Sidebar {
    pub(crate) fn new(notes: Entity<NoteStore>, cx: &mut Context<Self>) -> Self {
        let vault = notes.read(cx).vault().cloned();
        let watched = vault.map(|vault| cx.observe(&vault, |this, _, cx| this.rebuild(cx)));
        let mut sidebar = Self {
            notes,
            expanded: HashSet::new(),
            visible: Vec::new(),
            _vault_changed: watched,
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

    /// Expansion is the sidebar's own state; where a note opens is not. The
    /// sidebar does not know an editor exists.
    fn activate(&mut self, row: &Row, cx: &mut Context<Self>) {
        if row.kind == Kind::Dir {
            self.toggle(row.id, cx);
            return;
        }
        cx.emit(NoteActivated(row.id));
    }

    /// Split out only so a test can read the dot: nothing gets text back out
    /// of the `Div`s `render_rows` returns.
    fn row_label(&self, row: &Row, cx: &App) -> String {
        let dot = if self.notes.read(cx).is_dirty(row.id, cx) {
            " •"
        } else {
            ""
        };
        format!("{}{dot}", row.name)
    }

    /// `cx.processor` re-enters the sidebar entity here
    /// (`gpui/src/app/context.rs:260-267`); that is the only reason
    /// `cx.listener` is reachable from `uniform_list`'s `&mut App` closure.
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
                    // No explicit height: the row *is* its line box, so it
                    // cannot clip its own text however the type is set.
                    //
                    // That identity is also the only way the type statement
                    // below is under test. A fixed height would hold the row
                    // pitch at 22 px whatever the text did inside it, and a
                    // deleted `text_size`/`line_height` pair would silently
                    // restore the clipping this file used to have — gpui's
                    // default line box is `phi()` on a 16 px default, 25.89 px.
                    // Tied to the pitch, that deletion moves every row, and
                    // `a_click_far_down_the_list_lands_on_the_row_it_names`
                    // fails. Pixels the eye would catch become pixels the suite
                    // catches.
                    .text_size(theme::UI_TEXT_SIZE)
                    .line_height(theme::UI_LINE_HEIGHT)
                    .pl(INDENT_BASE + INDENT_STEP * depth)
                    // Load-bearing: row height is measured from item 0 alone
                    // (`elements/uniform_list.rs:658-680`), so a wrapping
                    // filename overflows its row.
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(theme::sidebar_text())
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

/// The vertical centre of row `ix`, for a test that clicks one.
///
/// Derived from the line height because the row has no height of its own: see
/// `render_rows`. A click computed here and landing elsewhere means the rendered
/// pitch has drifted from the stated one.
///
/// Lives beside the row height rather than in a test module, so `shell.rs` uses
/// the same definition: two suites clicking the sidebar must not each hard-code
/// the pitch and drift from it.
#[cfg(test)]
pub(crate) fn row_center(ix: f32) -> Pixels {
    theme::UI_LINE_HEIGHT * (ix + 0.5)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Instant;

    use gpui::{
        Entity, Modifiers, Subscription, TestAppContext, VisualTestContext, point, prelude::*, px,
    };
    use inkra_text::ByteOffset;
    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Vault};

    use super::{Kind, NoteActivated, Row, Sidebar, row_center};
    use crate::vault::NoteStore;

    /// Stops short of the sidebar: `add_window` constructs the view itself.
    fn parts(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new("/vault")))
            .expect("scan the test vault");
        let vault = cx.update(|cx| cx.new(|_| scanned));
        cx.update(|cx| cx.new(|cx| NoteStore::new(fs, Some(vault), cx)))
    }

    /// The same, with a sidebar that is never drawn.
    fn open(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<Sidebar> {
        open_with_store(cx, fs).0
    }

    fn open_with_store(
        cx: &mut TestAppContext,
        fs: Arc<dyn Fs>,
    ) -> (Entity<Sidebar>, Entity<NoteStore>) {
        let notes = parts(cx, fs);
        let sidebar = cx.update(|cx| cx.new(|cx| Sidebar::new(notes.clone(), cx)));
        (sidebar, notes)
    }

    /// A sidebar that *is* drawn, so a click has a layout to land in.
    fn drawn(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> (Entity<Sidebar>, VisualTestContext) {
        let notes = parts(cx, fs);
        let window = cx.add_window(move |_, cx| Sidebar::new(notes, cx));
        let sidebar = window.root(cx).expect("root view");
        (sidebar, VisualTestContext::from_window(window.into(), cx))
    }

    /// The conflict file's only route to the user, asserted where the user
    /// would see it: `visible`, not `Vault::entries`.
    ///
    /// `visible` is a cache rebuilt by `new` and `toggle` alone. A test that
    /// counts vault entries instead passes while the sidebar paints a stale
    /// row list, which is exactly what happened: the vault's `notify` had no
    /// subscriber, and the file this whole feature exists to surface was
    /// invisible until the next launch.
    #[gpui::test]
    fn a_file_appearing_in_the_vault_reaches_a_row(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        assert_eq!(sidebar.read_with(cx, |sidebar, _| sidebar.visible.len()), 1);

        // What a conflict leaves behind, and then the rescan that follows it.
        fs.insert_file("/vault/a (conflict 1970-01-01 00-00-01Z).md", "theirs");
        let vault = notes
            .read_with(cx, |notes, _| notes.vault().cloned())
            .expect("a vault");
        let children = futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault")))
            .expect("collect");
        vault.update(cx, |vault, cx| {
            vault.apply(children);
            cx.notify();
        });

        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.visible.len()),
            2,
            "the sidebar must observe the vault, not only its own toggles"
        );
    }

    /// Every `NoteActivated` the sidebar emits, in order.
    fn watch(
        sidebar: &Entity<Sidebar>,
        cx: &mut TestAppContext,
    ) -> (Rc<RefCell<Vec<NoteActivated>>>, Subscription) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let guard = cx.update(|cx| {
            let seen = Rc::clone(&seen);
            cx.subscribe(sidebar, move |_, event: &NoteActivated, _| {
                seen.borrow_mut().push(*event);
            })
        });
        (seen, guard)
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

        let sidebar = open(cx, fs as Arc<dyn Fs>);

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

        let sidebar = open(cx, fs as Arc<dyn Fs>);
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
    /// same press (`elements/uniform_list.rs:508-510`). The toggle side of
    /// that; the note side is `a_click_reports_the_row_it_landed_on`.
    #[gpui::test]
    fn a_press_on_one_row_does_not_reach_the_others(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "plan");
        fs.insert_file("/vault/Projects.md", "folder note");
        fs.insert_file("/vault/a.md", "alpha");

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);

        // The first row is the folder `Projects`.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();

        assert_eq!(
            labels(&sidebar, cx),
            ["Projects/", "plan", "Projects", "a"],
            "exactly one row responded"
        );
    }

    /// Subscribes to the `Sidebar` itself, so it proves nothing about
    /// `AppShell`'s wiring — that is `shell.rs`'s job.
    ///
    /// A real mouse event, not a call to `activate`: only the layout decides
    /// whether a click lands. Discriminates `.w_full()` — headless text is
    /// measured at 0.6 × `font_size` per glyph
    /// (`vendor/zed-reference/crates/gpui/src/platform.rs:1288-1296`), so an
    /// `auto` row around the stem `a` ends at 17.6px and x=20 misses.
    ///
    /// Does *not* cover `.size_full()`: a window root resolves its percentage
    /// against the test display's definite bounds, never the real `SIDEBAR_WIDTH` parent
    /// in `shell.rs`. The folder keeps a sibling note so that clicking it
    /// proves no *other* row reported either.
    #[gpui::test]
    fn a_click_reports_the_row_it_landed_on(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/Projects/plan.md", "plan");
        fs.insert_file("/vault/a.md", "alpha");

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
        let (seen, _guard) = watch(&sidebar, cx);

        // Row 0 is the folder `Projects`.
        visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
        visual.run_until_parked();
        assert!(
            seen.borrow().is_empty(),
            "a folder reports no note, its own or a sibling's"
        );

        // Expanded, so `a` is now row 2: `Projects/`, `plan`, `a`.
        let a = row(&sidebar, cx, "a");
        visual.simulate_click(point(px(20.), row_center(2.)), Modifiers::none());
        visual.run_until_parked();
        assert_eq!(*seen.borrow(), [NoteActivated(a.id)]);
    }

    /// The row pitch, asserted where a drift has somewhere to show.
    ///
    /// `render_rows` states the sidebar's type and lets each row take its line
    /// box as its height. Nothing but that identity holds the two together, so
    /// this is the test the type statement lives or dies by: a click computed
    /// from `UI_LINE_HEIGHT` has to land on the row that constant names.
    ///
    /// Thirty rows is what makes it bite, and the depth is the whole design.
    /// The two pitches below are **measured**, not derived — deleting the pair
    /// and probing where row 1 begins gives 26.0 px, and deleting `line_height`
    /// alone gives 22.5 px. (The second is not `phi()` times 14, which would be
    /// 22.65; gpui does not render the fraction it computes.) Against the stated
    /// 22 px, 26.0 first misplaces a click at row 3, and 22.5 — half a pixel of
    /// drift per row — not until row 23. Clicking row 29 catches both with six
    /// rows in hand. Twenty rows caught only the first.
    ///
    /// What it pins is `line_height`, which is the row's height and therefore
    /// the only thing that can make a line box outgrow its row. Deleting
    /// `text_size` alone leaves this green, and should: 16 px inside a stated
    /// 22 px box still fits, so that is a typographic regression and not the
    /// clipping defect. Nothing here catches it, and nothing here claims to.
    ///
    /// The alternative was a fixed `.h()` on the row, which is what this file
    /// used to have. It holds the pitch at 22 px whatever the text does inside
    /// it — so the clipping the type statement exists to prevent comes back
    /// silently, and every test here still passes. That is the state this test
    /// exists to leave behind.
    #[gpui::test]
    fn a_click_far_down_the_list_lands_on_the_row_it_names(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        // Two digits, so the comparator's lexical order is the numeric one:
        // `note-9` would sort between `note-19` and `note-2`.
        for ix in 0..30 {
            fs.insert_file(format!("/vault/note-{ix:02}.md"), "text");
        }

        let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
        let (seen, _guard) = watch(&sidebar, cx);

        let last = row(&sidebar, cx, "note-29");
        visual.simulate_click(point(px(20.), row_center(29.)), Modifiers::none());
        visual.run_until_parked();

        assert_eq!(
            *seen.borrow(),
            [NoteActivated(last.id)],
            "the rendered pitch has drifted from the stated line height"
        );
    }

    /// Asserts the label, not `is_dirty`: the predicate passes with the dot
    /// deleted. One note edited, so a marker on every row also fails.
    #[gpui::test]
    async fn an_edited_note_is_marked_in_the_tree_until_it_is_saved(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "alpha");
        fs.insert_file("/vault/b.md", "beta");

        let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let b = row(&sidebar, cx, "b");

        let opened = notes.update(cx, |notes, cx| notes.open_note(b.id, cx));
        let buffer = opened.await.expect("`b` opens");
        notes.update(cx, |notes, cx| notes.set_current(b.id, cx));
        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "opening a note changes nothing about it"
        );

        buffer.update(cx, |buffer, _| {
            buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), "X"));
        });

        assert_eq!(row_labels(&sidebar, cx), ["a", "b •"]);

        notes.update(cx, NoteStore::save_current);
        cx.run_until_parked();

        assert_eq!(
            row_labels(&sidebar, cx),
            ["a", "b"],
            "the write caught the file up, so the note is no longer modified"
        );
    }
}
