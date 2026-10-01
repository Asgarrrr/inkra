#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind};

use super::tests::{drawn, row};
use crate::theme;

fn bounds(visual: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    visual.run_until_parked();
    visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("`{selector}` is drawn"))
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
