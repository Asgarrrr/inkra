//! Hiding and showing the docks, their toggles and their headers (docks spec).

use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext, px, size};
use inkra_vault::mem_fs::MemFs;

use super::tab_bar::click;
use super::{
    ab_focused, editor_focused, focus_tree, keys, no_vault, open, selected, shell_focused,
    tree_focused, two_notes, vault_names,
};
use crate::test_support::entry_id;

fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn cmd_backslash_hides_and_shows_the_sidebar(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(drawn(&mut cx, "dock-left"));

    keys(&mut cx, "cmd-\\");
    assert!(!drawn(&mut cx, "dock-left"));
    assert!(drawn(&mut cx, "dock-right"));
    let row = cx.debug_bounds("tab-row").expect("the tab row is drawn");
    assert_eq!(row.left(), px(0.), "the note column takes the width");

    keys(&mut cx, "cmd-\\");
    assert!(drawn(&mut cx, "dock-left"));
}

/// `cmd-|` is what macOS delivers for ⌘⇧\; a `cmd-shift-\` binding would never fire.
#[gpui::test]
fn cmd_pipe_hides_and_shows_the_backlinks(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(drawn(&mut cx, "dock-right"));

    keys(&mut cx, "cmd-|");
    assert!(!drawn(&mut cx, "dock-right"));
    assert!(drawn(&mut cx, "dock-left"));

    keys(&mut cx, "cmd-|");
    assert!(drawn(&mut cx, "dock-right"));
}

#[gpui::test]
fn hiding_the_focused_sidebar_lands_focus_on_the_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);
    assert!(!editor_focused(&window, &mut cx));

    keys(&mut cx, "cmd-\\");

    assert!(editor_focused(&window, &mut cx));
}

/// A toggle inside the scrolling `div` shifts `scroll_to_item`'s index by one (`div.rs:4291`).
#[gpui::test]
fn the_tab_bar_still_scrolls_to_the_active_tab_with_toggles(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    keys(&mut cx, "cmd-9");

    let bar = cx.debug_bounds("tab-bar").unwrap();
    let left = cx.debug_bounds("dock-toggle-left").unwrap();
    let right = cx.debug_bounds("dock-toggle-right").unwrap();
    assert!(left.right() <= bar.left() && bar.right() <= right.left());
    let last = cx.debug_bounds("tab-29").unwrap();
    assert!(
        last.left() >= bar.left() && last.right() <= bar.right(),
        "tab-29 at {last:?} is outside the bar at {bar:?}"
    );
}

#[gpui::test]
fn a_shown_sidebar_keeps_its_open_folders_and_selection(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/projects/inkra.md", "inkra");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = entry_id(&notes, &mut cx, "a");
    window
        .update(&mut cx, |shell, _, cx| {
            shell.toggle_tree_row("projects", cx);
            shell
                .panes()
                .sidebar
                .update(cx, |sidebar, cx| sidebar.select_for_test(a, cx));
        })
        .unwrap();
    cx.run_until_parked();

    keys(&mut cx, "cmd-\\");
    keys(&mut cx, "cmd-\\");

    let names = window
        .update(&mut cx, |shell, _, cx| shell.tree_row_names(cx))
        .unwrap();
    assert_eq!(names, ["projects", "inkra", "a"]);
    assert_eq!(selected(&window, &mut cx), Some(a));
}

/// Cancelled, not committed: a half-typed name must not create a note.
#[gpui::test]
fn hiding_the_sidebar_cancels_an_open_name_field(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, _a, _b) = two_notes(cx, &fs);
    let mut visual = tree_focused(cx, window, &sidebar);
    keys(&mut visual, "cmd-n p l a n");

    keys(&mut visual, "cmd-\\");
    keys(&mut visual, "cmd-\\");

    assert!(!sidebar.read_with(&visual, |sidebar, _| sidebar.is_editing()));
    assert_eq!(vault_names(&notes, cx), ["a.md", "b.md"]);
}

/// The editor keeps focus: a press on a toggle must not hand it to the shell's root.
#[gpui::test]
fn a_click_on_a_toggle_hides_its_panel(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    for (toggle, column) in [
        ("dock-toggle-left", "dock-left"),
        ("dock-toggle-right", "dock-right"),
    ] {
        click(&mut cx, toggle);
        assert!(!drawn(&mut cx, column), "{toggle}");
        assert!(editor_focused(&window, &mut cx), "{toggle}");

        click(&mut cx, toggle);
        assert!(drawn(&mut cx, column), "{toggle}");
    }
}

#[gpui::test]
fn no_vault_shows_no_toggle(cx: &mut TestAppContext) {
    let (_window, mut cx) = no_vault(cx);
    assert!(drawn(&mut cx, "tab-row"));
    assert!(!drawn(&mut cx, "dock-toggle-left"));
    assert!(!drawn(&mut cx, "dock-toggle-right"));
}

/// An undrawn field would take focus and every keystroke, out of sight.
#[gpui::test]
fn creating_with_the_sidebar_hidden_shows_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-\\");
    assert!(!drawn(&mut cx, "dock-left"));

    keys(&mut cx, "cmd-n");

    assert!(drawn(&mut cx, "dock-left"));
    let sidebar = window
        .update(&mut cx, |shell, _, _| shell.panes().sidebar.clone())
        .unwrap();
    let item = sidebar.read_with(&cx, |sidebar, _| sidebar.field_item());
    assert!(drawn(&mut cx, format!("tree-row-{item}").leak()));
    let focused = window
        .update(&mut cx, |_, window, cx| {
            sidebar.read(cx).field_is_focused(window, cx)
        })
        .unwrap();
    assert!(focused);
    keys(&mut cx, "p l a n enter");
    let notes = window
        .update(&mut cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(vault_names(&notes, &mut cx), ["a.md", "b.md", "plan.md"]);
}

/// Edges included: `Bounds::is_contained_within` excludes the right and bottom ones.
fn inside(cx: &mut VisualTestContext, inner: &'static str, outer: &'static str) -> bool {
    let inner = cx.debug_bounds(inner).expect("drawn");
    let outer = cx.debug_bounds(outer).expect("drawn");
    outer.left() <= inner.left()
        && inner.right() <= outer.right()
        && outer.top() <= inner.top()
        && inner.bottom() <= outer.bottom()
}

#[gpui::test]
fn the_files_header_tops_the_left_dock(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(inside(&mut cx, "panel-header-files", "dock-left"));
    let dock = cx.debug_bounds("dock-left").unwrap();
    let header = cx.debug_bounds("panel-header-files").unwrap();
    assert_eq!(header.top(), dock.top());
}

#[gpui::test]
fn the_backlinks_header_is_drawn_by_the_dock(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    assert!(inside(&mut cx, "panel-header-backlinks", "dock-right"));
}

/// A header takes no focus: the root would take it on the press.
#[gpui::test]
fn a_press_on_a_dock_header_keeps_the_editor_focused(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    assert!(editor_focused(&window, &mut cx));

    click(&mut cx, "panel-header-files");

    assert!(editor_focused(&window, &mut cx));
}
