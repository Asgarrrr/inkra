#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    Context, Entity, Modifiers, Subscription, TestAppContext, VisualTestContext, point, prelude::*,
    px,
};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use super::edit::{EditState, Rejection};
use super::{Kind, NoteActivated, Row, Sidebar, row_center};
use crate::vault::NoteStore;

impl Sidebar {
    /// Selects a row without a window, for a test that has none.
    #[cfg(test)]
    pub(crate) fn select_for_test(&mut self, id: EntryId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    /// Opens a rename field without focusing it. `#[cfg(test)]`: the real entry
    /// point needs a `Window`, and these tests exercise the rebuild rather than
    /// the gesture.
    #[cfg(test)]
    pub(crate) fn rename_for_test(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        let Some(at) = self.index_of(id) else { return };
        let row = self.visible[at].clone();
        let parent = self.parent_of(&row);
        let (field, subscriptions) = Self::open_field(&row.name, cx);
        self.editing = Some(EditState {
            target: Some(id),
            parent,
            kind: row.kind,
            at,
            depth: row.depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
    }

    /// The same, for a create.
    #[cfg(test)]
    pub(crate) fn create_for_test(&mut self, cx: &mut Context<Self>) {
        let (parent, at, depth) = self.insertion_point(cx);
        let (field, subscriptions) = Self::open_field("", cx);
        self.editing = Some(EditState {
            target: None,
            parent,
            kind: Kind::Note,
            at,
            depth,
            field,
            rejected: None,
            _subscriptions: subscriptions,
        });
    }

    /// Where the field's row sits, for the shrink tests.
    #[cfg(test)]
    pub(crate) fn field_row(&self) -> usize {
        self.editing.as_ref().map_or(0, |edit| edit.at)
    }

    /// Whether a name is being typed. `#[cfg(test)]` for the same reason as
    /// `selected`: nothing in the UI asks, and an unread method fails
    /// `-D warnings`.
    #[cfg(test)]
    pub(crate) fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// Why the last confirm was refused, as the user would read it. Asserting
    /// the sentence rather than the variant is what stops a rejection being
    /// reported with the wrong reason.
    #[cfg(test)]
    pub(crate) fn rejection_message(&self) -> Option<&'static str> {
        self.editing
            .as_ref()
            .and_then(|edit| edit.rejected)
            .map(Rejection::message)
    }

    /// The row a command would act on. `#[cfg(test)]` and an accessor rather
    /// than a public field, following `NoteStore::current`: nothing in the UI
    /// asks, and an unread method fails `-D warnings`.
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<EntryId> {
        self.selected
    }
}

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

/// A walk landing while a name is typed can leave the field's row past the
/// end of the tree. An unrendered row has no dispatch node, so the field
/// would take no keystrokes at all — not even `escape`.
#[gpui::test]
fn a_field_survives_the_tree_shrinking_under_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.insert_file("/vault/c.md", "gamma");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let last = row(&sidebar, cx, "c");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
    sidebar.update(cx, Sidebar::rename_for_test);
    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));

    // Two notes vanish from outside, so the field's row is past the end.
    fs.remove(Path::new("/vault/b.md"));
    fs.remove(Path::new("/vault/c.md"));
    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    let children =
        futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault"))).expect("collect");
    vault.update(cx, |vault, cx| {
        vault.apply(children);
        cx.notify();
    });

    assert!(
        !sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
        "the renamed entry is gone, so the gesture has no target left"
    );
}

/// The same shrink, with a create: the field has no target to lose, so it
/// stays open — and its row has to be one the list still draws.
#[gpui::test]
fn a_create_field_is_pulled_back_inside_the_tree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let last = row(&sidebar, cx, "b");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
    sidebar.update(cx, Sidebar::create_for_test);

    fs.remove(Path::new("/vault/b.md"));
    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    let children =
        futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault"))).expect("collect");
    vault.update(cx, |vault, cx| {
        vault.apply(children);
        cx.notify();
    });

    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
    assert!(
        sidebar.read_with(cx, |sidebar, _| sidebar.field_row() < sidebar.row_count()),
        "the field's row has to be one the list draws"
    );
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
    let children =
        futures::executor::block_on(Vault::collect(&*fs, Path::new("/vault"))).expect("collect");
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
/// The alternative was a fixed `.h()` on the row, which is what the row
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
