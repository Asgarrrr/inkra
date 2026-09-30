#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use futures::FutureExt;
use gpui::{
    Context, Entity, Focusable, Modifiers, Subscription, TestAppContext, VisualTestContext, point,
    prelude::*, px,
};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use super::edit::Rejection;
use super::rows::Item;
use super::{Kind, Row, Sidebar, row_center};
use crate::open::{NoteActivated, Open};
use crate::test_support::opened;
use crate::vault::NoteStore;

impl Sidebar {
    #[cfg(test)]
    pub(crate) fn select_for_test(&mut self, id: EntryId, cx: &mut Context<Self>) {
        self.select(Some(id));
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn field_item(&self) -> usize {
        self.rows.field_item().unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn is_editing(&self) -> bool {
        self.rows.edit().is_some()
    }

    #[cfg(test)]
    pub(crate) fn field_is_focused(&self, window: &gpui::Window, cx: &gpui::App) -> bool {
        self.rows
            .edit()
            .is_some_and(|edit| edit.field.focus_handle(cx).is_focused(window))
    }

    /// Asserts the sentence, not the variant: that is what catches a rejection reported with the wrong reason.
    #[cfg(test)]
    pub(crate) fn rejection_message(&self) -> Option<&'static str> {
        self.rows
            .edit()
            .and_then(|edit| edit.rejected)
            .map(Rejection::message)
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<EntryId> {
        self.selected
    }

    fn visible_rows(&self) -> Vec<Row> {
        let visible = self.rows.visible();
        (0..visible.len()).map(|ix| visible.row(ix)).collect()
    }

    /// The row an item draws; `None` for the field.
    fn row_at(&self, item: usize) -> Option<Row> {
        match self.rows.item(item)? {
            Item::Row(row) => Some(self.rows.visible().row(row)),
            Item::Field(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn row_names(&self) -> Vec<String> {
        self.visible_rows()
            .into_iter()
            .map(|row| row.name)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn toggle_named(&mut self, name: &str, cx: &mut Context<Self>) {
        let id = self
            .visible_rows()
            .into_iter()
            .find(|row| row.name == name)
            .map(|row| row.id)
            .expect("the row is visible");
        self.toggle(id, cx);
    }
}

fn parts(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
    crate::test_support::scan_notes(cx, fs)
}

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

fn drawn(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> (Entity<Sidebar>, VisualTestContext) {
    let notes = parts(cx, fs);
    let window = cx.add_window(move |_, cx| Sidebar::new(notes, cx));
    let sidebar = window.root(cx).expect("root view");
    (sidebar, VisualTestContext::from_window(window.into(), cx))
}

/// An unrendered row has no dispatch node, so the field would take no keystrokes — not even `escape`.
#[gpui::test]
fn a_field_survives_the_tree_shrinking_under_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.insert_file("/vault/c.md", "gamma");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let last = row(&sidebar, cx, "c");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
    sidebar.update(cx, |sidebar, cx| {
        sidebar.open_rename(cx);
    });
    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));

    fs.remove(Path::new("/vault/b.md"));
    fs.remove(Path::new("/vault/c.md"));
    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    vault.update(cx, |vault, cx| {
        futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
        cx.notify();
    });

    assert!(
        !sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()),
        "the renamed entry is gone, so the gesture has no target left"
    );
}

/// A real scan swaps the vault in, through the store's worker, under an open rename field.
#[gpui::test]
fn a_rename_field_follows_its_row_across_a_scans_swap(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let b = row(&sidebar, cx, "b");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.select_for_test(b.id, cx);
        sidebar.open_rename(cx);
    });
    assert_eq!(sidebar.read_with(cx, |sidebar, _| sidebar.field_item()), 1);

    // Sorts first, so the swap moves `b` down a row.
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "0", cx));
    cx.run_until_parked();

    assert!(
        created.now_or_never().flatten().is_some(),
        "the scan swapped in"
    );
    assert_eq!(labels(&sidebar, cx), ["0", "a", "b"]);
    sidebar.read_with(cx, |sidebar, _| {
        assert!(sidebar.is_editing());
        assert_eq!(sidebar.rows.target(), Some(b.id));
        assert_eq!(sidebar.field_item(), 2);
    });
}

/// A create has no target to lose, so the field stays open on a row the list still draws.
#[gpui::test]
fn a_create_field_is_pulled_back_inside_the_tree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let last = row(&sidebar, cx, "b");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(last.id, cx));
    sidebar.update(cx, |sidebar, cx| {
        sidebar.open_create(Kind::Note, cx);
    });

    fs.remove(Path::new("/vault/b.md"));
    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    vault.update(cx, |vault, cx| {
        futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
        cx.notify();
    });

    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
    assert!(
        sidebar.read_with(cx, |sidebar, _| sidebar.field_item() < sidebar.item_count()),
        "the field's row has to be one the list draws"
    );
}

/// Asserted on `visible`, not `Vault::entries`: `visible` is a cache rebuilt
/// only by `new` and `toggle`, so this pins the vault subscription.
#[gpui::test]
fn a_file_appearing_in_the_vault_reaches_a_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rows.visible().len()),
        1
    );

    fs.insert_file("/vault/a (conflict 1970-01-01 00-00-01Z).md", "theirs");
    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    vault.update(cx, |vault, cx| {
        futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
        cx.notify();
    });

    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rows.visible().len()),
        2,
        "the sidebar must observe the vault, not only its own toggles"
    );
}

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

/// Trailing `/` on directories: `Projects` and `Projects.md` are otherwise indistinguishable.
fn labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
    sidebar.update(cx, |sidebar, _| {
        sidebar.visible_rows().iter().map(label).collect()
    })
}

fn label(row: &Row) -> String {
    match row.kind {
        Kind::Dir => format!("{}/", row.name),
        Kind::Note => row.name.clone(),
    }
}

fn row_labels(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext) -> Vec<String> {
    sidebar.update(cx, |sidebar, cx| {
        sidebar
            .visible_rows()
            .iter()
            .map(|row| sidebar.row_label(row, cx))
            .collect()
    })
}

fn row(sidebar: &Entity<Sidebar>, cx: &mut TestAppContext, wanted: &str) -> Row {
    sidebar.update(cx, |sidebar, _| {
        sidebar
            .visible_rows()
            .into_iter()
            .find(|row| label(row) == wanted)
            .expect("the row is visible")
    })
}

#[gpui::test]
fn collapsing_a_folder_hides_its_children_and_nothing_else(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "p");
    fs.insert_file("/vault/Projects.md", "n");
    fs.insert_file("/vault/a.md", "a");

    let sidebar = open(cx, fs as Arc<dyn Fs>);

    assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);

    let projects = row(&sidebar, cx, "Projects/");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.activate(&projects, Open::Here, cx);
    });
    assert_eq!(labels(&sidebar, cx), ["Projects/", "plan", "Projects", "a"]);

    sidebar.update(cx, |sidebar, cx| {
        sidebar.activate(&projects, Open::Here, cx);
    });
    assert_eq!(labels(&sidebar, cx), ["Projects/", "Projects", "a"]);
}

#[gpui::test]
fn expanding_a_folder_leaves_its_nested_folders_collapsed(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/deep/x.md", "x");
    fs.insert_file("/vault/Projects/plan.md", "p");

    let sidebar = open(cx, fs as Arc<dyn Fs>);
    assert_eq!(labels(&sidebar, cx), ["Projects/"]);

    let projects = row(&sidebar, cx, "Projects/");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.activate(&projects, Open::Here, cx);
    });

    assert_eq!(
        labels(&sidebar, cx),
        ["Projects/", "deep/", "plan"],
        "`x` lives inside `deep`, which nobody expanded"
    );
}

/// The toggle side; the note side is `a_click_reports_the_row_it_landed_on`.
#[gpui::test]
fn a_press_on_one_row_does_not_reach_the_others(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "plan");
    fs.insert_file("/vault/Projects.md", "folder note");
    fs.insert_file("/vault/a.md", "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);

    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(
        labels(&sidebar, cx),
        ["Projects/", "plan", "Projects", "a"],
        "exactly one row responded"
    );
}

/// Discriminates `.w_full()`: headless text measures at 0.6 x `font_size`
/// per glyph, so an `auto` row around the stem `a` ends at 17.6px and x=20
/// misses. Does not cover `.size_full()`, which needs the real `SIDEBAR_WIDTH`
/// parent from `shell.rs` to resolve against.
#[gpui::test]
fn a_click_reports_the_row_it_landed_on(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "plan");
    fs.insert_file("/vault/a.md", "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let (seen, _guard) = watch(&sidebar, cx);

    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::none());
    visual.run_until_parked();
    assert!(
        seen.borrow().is_empty(),
        "a folder reports no note, its own or a sibling's"
    );

    // Expanded: `a` is now row 2 (`Projects/`, `plan`, `a`).
    let a = row(&sidebar, cx, "a");
    visual.simulate_click(point(px(20.), row_center(2.)), Modifiers::none());
    visual.run_until_parked();
    assert_eq!(
        *seen.borrow(),
        [NoteActivated {
            note: a.id,
            open: Open::Here
        }]
    );
}

/// Pinned at row 29 of 30 so drift passes a whole row before this catches
/// it: deleting `line_height` alone first misplaces a click at row 3, and
/// deleting `text_size` alone stays green correctly (16px still fits).
#[gpui::test]
fn a_click_far_down_the_list_lands_on_the_row_it_names(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    // Zero-padded so the comparator's lexical order is the numeric one.
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
        [NoteActivated {
            note: last.id,
            open: Open::Here
        }],
        "the rendered pitch has drifted from the stated line height"
    );
}

/// Asserts the label, not `is_dirty`: that predicate passes even with the dot deleted from render.
#[gpui::test]
async fn an_edited_note_is_marked_in_the_tree_until_it_is_saved(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");

    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let b = row(&sidebar, cx, "b");

    let buffer = opened(&notes, cx, b.id).await;
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

#[gpui::test]
fn a_cmd_click_on_a_note_row_activates_it_in_a_new_tab(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let (seen, _guard) = watch(&sidebar, cx);

    let a = row(&sidebar, cx, "a");
    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::command());
    visual.run_until_parked();

    assert_eq!(
        *seen.borrow(),
        [NoteActivated {
            note: a.id,
            open: Open::NewTab
        }]
    );
}

#[gpui::test]
fn a_cmd_click_on_a_folder_row_toggles_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "plan");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let (seen, _guard) = watch(&sidebar, cx);

    visual.simulate_click(point(px(20.), row_center(0.)), Modifiers::command());
    visual.run_until_parked();

    assert_eq!(labels(&sidebar, cx), ["Projects/", "plan"]);
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn a_click_during_a_rename_does_not_activate_another_row(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let (seen, _guard) = watch(&sidebar, cx);
    let a = row(&sidebar, cx, "a");
    sidebar.update(cx, |sidebar, cx| sidebar.select_for_test(a.id, cx));
    visual.update(|window, cx| {
        sidebar.update(cx, |sidebar, cx| sidebar.rename(&super::Rename, window, cx));
    });
    visual.run_until_parked();

    visual.simulate_click(point(px(20.), row_center(1.)), Modifiers::none());
    visual.run_until_parked();

    assert!(seen.borrow().is_empty(), "no row activates");
    let (editing, selected) =
        sidebar.read_with(cx, |sidebar, _| (sidebar.is_editing(), sidebar.selected()));
    assert!(editing, "the rename is still open");
    assert_eq!(selected, Some(a.id), "on the row it renames");
}

/// A file name may hold a newline on macOS; `uniform_list` sizes every row from row 0.
#[gpui::test]
fn a_newline_in_a_file_name_keeps_the_row_one_line(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a\nb.md", "text");
    fs.insert_file("/vault/c.md", "text");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let (seen, _guard) = watch(&sidebar, cx);

    let c = row(&sidebar, cx, "c");
    visual.simulate_click(point(px(20.), row_center(1.)), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(
        *seen.borrow(),
        [NoteActivated {
            note: c.id,
            open: Open::Here
        }],
        "a two-line row 0 covers row 1"
    );
}

#[gpui::test]
fn the_rows_below_a_create_field_shift_down_by_one(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.insert_file("/vault/c.md", "gamma");
    let sidebar = open(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let first = row(&sidebar, cx, "a");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.select_for_test(first.id, cx);
        sidebar.open_create(Kind::Note, cx);
    });

    let rows: Vec<Option<String>> = sidebar.read_with(cx, |sidebar, _| {
        (0..sidebar.item_count())
            .map(|ix| sidebar.row_at(ix).map(|row| row.name))
            .collect()
    });
    let expected = [Some("a"), None, Some("b"), Some("c")].map(|name| name.map(str::to_owned));
    assert_eq!(rows, expected, "the field takes row 1");
}

/// A hidden target has no row, so the field closes rather than land on an undrawn one.
#[gpui::test]
fn a_rename_field_closes_when_its_note_moves_under_a_collapsed_folder(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Archive/old.md", "o");
    fs.insert_file("/vault/a.md", "alpha");
    let (sidebar, notes) = open_with_store(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = row(&sidebar, cx, "a");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.select_for_test(a.id, cx);
        sidebar.open_rename(cx);
    });
    assert!(sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));

    let vault = notes
        .read_with(cx, |notes, _| notes.vault().cloned())
        .expect("a vault");
    vault.update(cx, |vault, cx| {
        futures::executor::block_on(
            fs.rename(Path::new("/vault/a.md"), Path::new("/vault/Archive/a.md")),
        )
        .expect("rename");
        futures::executor::block_on(vault.rescan(&*fs)).expect("rescan");
        cx.notify();
    });

    assert!(
        notes.read_with(cx, |notes, cx| notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .any(|entry| entry.id == a.id)),
        "the move keeps the note's id, so only the collapse hides it"
    );
    assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.is_editing()));
}
