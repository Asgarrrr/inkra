#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, px, size};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind};

use super::tests::{drawn, row};
use crate::test_support::{edit, opened};
use crate::theme;
use crate::vault::NoteStore;

fn bounds(visual: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    visual.run_until_parked();
    let Some(bounds) = visual.debug_bounds(selector) else {
        panic!("`{selector}` is drawn");
    };
    bounds
}

#[gpui::test]
fn a_note_and_a_folder_name_start_at_the_same_x(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "plan");
    fs.insert_file("/vault/a.md", "alpha");

    let (_sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);

    let folder = bounds(&mut visual, "tree-name-0");
    let note = bounds(&mut visual, "tree-name-1");
    assert_eq!(folder.left(), note.left());
}

#[gpui::test]
fn a_child_name_starts_one_step_right(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/Projects/plan.md", "plan");
    fs.insert_file("/vault/a.md", "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    sidebar.update(cx, |sidebar, cx| sidebar.toggle_named("Projects", cx));

    // `Projects/`, `plan`, `a`: two notes, one level apart.
    let child = bounds(&mut visual, "tree-name-1");
    let root = bounds(&mut visual, "tree-name-2");
    assert_eq!(child.left() - root.left(), theme::INDENT_STEP);
}

#[gpui::test]
fn the_field_row_has_the_tree_rows_height(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let a = row(&sidebar, cx, "a");
    sidebar.update(cx, |sidebar, cx| {
        sidebar.select_for_test(a.id, cx);
        sidebar.open_create(Kind::Note, cx);
    });

    let plain = bounds(&mut visual, "tree-row-0");
    let field = bounds(&mut visual, "tree-row-1");
    assert_eq!(field.size.height, plain.size.height);
}

/// The shell's sidebar column is 220 px wide; a nowrap name inside the content cell would push the dot out.
#[gpui::test]
async fn a_dirty_rows_dot_survives_a_long_name(cx: &mut TestAppContext) {
    let name = "n".repeat(200);
    let fs = Arc::new(MemFs::new());
    fs.insert_file(format!("/vault/{name}.md"), "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    visual.simulate_resize(size(px(220.), px(400.)));
    let note = row(&sidebar, cx, &name);
    let notes = sidebar.read_with(cx, |sidebar, _| sidebar.notes.clone());
    let buffer = opened(&notes, cx, note.id).await;
    edit(&buffer, cx, "X");

    let dot = bounds(&mut visual, "tree-dot-0");
    let row = bounds(&mut visual, "tree-row-0");
    assert!(
        dot.right() <= row.right(),
        "the dot at {dot:?} is outside the row at {row:?}"
    );
}

/// No view is cached, so the save's notify redraws the tree with no further input (spec S1-D4).
#[gpui::test]
async fn a_saved_notes_dot_leaves_the_frame(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");

    let (sidebar, mut visual) = drawn(cx, fs as Arc<dyn Fs>);
    let a = row(&sidebar, cx, "a");
    let notes = sidebar.read_with(cx, |sidebar, _| sidebar.notes.clone());
    let buffer = opened(&notes, cx, a.id).await;
    edit(&buffer, cx, "X");
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("tree-dot-0").is_some(),
        "the edit draws it"
    );

    notes.update(cx, NoteStore::save_current);
    visual.run_until_parked();

    assert!(
        visual.debug_bounds("tree-dot-0").is_none(),
        "the save clears it"
    );
}
