//! Hiding and showing the docks, their toggles and their headers (docks spec).

use std::sync::Arc;
use std::time::Duration;

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle, px, size};
use inkra_layout::{Side, View};
use inkra_vault::mem_fs::MemFs;

use super::dock_menu::docks;
use super::dragging::drag;
use super::tab_bar::click;
use super::{
    ab_focused, editor_focused, focus_tree, keys, no_vault, open, selected, shell_focused,
    tree_focused, two_notes, vault_names,
};
use crate::shell::AppShell;
use crate::shell::docks::{MoveBacklinksToOtherDock, MoveFilesToOtherDock};
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
pub(super) fn inside(cx: &mut VisualTestContext, inner: &'static str, outer: &'static str) -> bool {
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

fn move_view(cx: &mut VisualTestContext, view: View) {
    match view {
        View::Files => cx.dispatch_action(MoveFilesToOtherDock),
        View::Backlinks => cx.dispatch_action(MoveBacklinksToOtherDock),
    }
    cx.run_until_parked();
}

fn active(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, side: Side) -> Option<View> {
    let [left, right] = docks(window, cx);
    match side {
        Side::Left => left.1,
        Side::Right => right.1,
    }
}

#[gpui::test]
fn moving_backlinks_left_draws_an_icon_bar_and_no_right_dock(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);

    move_view(&mut cx, View::Backlinks);

    for shown in ["dock-bar-left", "dock-icon-files", "dock-icon-backlinks"] {
        assert!(drawn(&mut cx, shown), "{shown}");
    }
    assert!(inside(&mut cx, "dock-icon-backlinks", "dock-bar-left"));
    for hidden in ["dock-right", "title-right", "dock-toggle-right"] {
        assert!(!drawn(&mut cx, hidden), "{hidden}");
    }
}

#[gpui::test]
fn a_click_on_an_inactive_icon_activates_its_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    move_view(&mut cx, View::Backlinks);
    assert_eq!(active(&window, &mut cx, Side::Left), Some(View::Backlinks));

    click(&mut cx, "dock-icon-files");

    assert_eq!(active(&window, &mut cx, Side::Left), Some(View::Files));
    assert!(drawn(&mut cx, "tree-row-0"));
}

#[gpui::test]
fn a_click_on_the_active_icon_changes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    move_view(&mut cx, View::Backlinks);
    let before = docks(&window, &mut cx);

    click(&mut cx, "dock-icon-backlinks");

    let after = docks(&window, &mut cx);
    assert_eq!(after, before);
}

#[gpui::test]
fn an_icon_shows_its_name_as_a_tooltip(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    move_view(&mut cx, View::Backlinks);
    let icon = cx.debug_bounds("dock-icon-backlinks").expect("drawn");

    cx.simulate_mouse_move(icon.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();

    assert!(drawn(&mut cx, "tooltip-Backlinks"));
}

#[gpui::test]
fn a_focused_tree_keeps_focus_when_it_moves(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);

    move_view(&mut cx, View::Files);

    assert!(inside(&mut cx, "dock-icon-files", "title-right"));
    assert_eq!(active(&window, &mut cx, Side::Right), Some(View::Files));
    let focused = window
        .update(&mut cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .unwrap();
    assert!(focused);
}

#[gpui::test]
fn covering_the_focused_tree_lands_focus_on_the_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);

    move_view(&mut cx, View::Backlinks);

    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn an_empty_left_dock_draws_no_toggle(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);

    move_view(&mut cx, View::Files);

    for hidden in ["dock-left", "title-left", "dock-toggle-left"] {
        assert!(!drawn(&mut cx, hidden), "{hidden}");
    }
    assert!(drawn(&mut cx, "title-drag-start"));
}

#[gpui::test]
fn the_dock_icon_point_lands_on_the_icon(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let lands = |cx: &mut VisualTestContext, selector: &'static str, side, index| {
        let bounds = cx.debug_bounds(selector).expect("drawn");
        let at = cx.update(|window, _| AppShell::dock_icon_point(side, index, window));
        assert!(
            bounds.contains(&at),
            "{selector}: {at:?} outside {bounds:?}"
        );
    };
    lands(&mut cx, "dock-icon-files", Side::Left, 0);

    move_view(&mut cx, View::Backlinks);
    lands(&mut cx, "dock-icon-files", Side::Left, 0);
    lands(&mut cx, "dock-icon-backlinks", Side::Left, 1);

    move_view(&mut cx, View::Files);
    move_view(&mut cx, View::Backlinks);
    lands(&mut cx, "dock-icon-files", Side::Right, 0);
    lands(&mut cx, "dock-icon-backlinks", Side::Right, 1);
}

#[gpui::test]
fn a_reorder_inside_a_dock_keeps_the_active_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    move_view(&mut cx, View::Backlinks);
    click(&mut cx, "dock-icon-files");

    drag(&mut cx, "dock-icon-backlinks", "dock-icon-files");

    let [(views, active, _), _] = docks(&window, &mut cx);
    assert_eq!(views, [View::Backlinks, View::Files]);
    assert_eq!(active, Some(View::Files));
}

#[gpui::test]
fn a_view_moved_to_the_other_dock_becomes_active_there(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    keys(&mut cx, "cmd-|");

    move_view(&mut cx, View::Files);

    let [_, right] = docks(&window, &mut cx);
    assert_eq!(
        right,
        (vec![View::Backlinks, View::Files], Some(View::Files), true)
    );
}
