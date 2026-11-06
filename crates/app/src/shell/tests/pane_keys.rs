//! The `⌘K` keys: a split in each direction, and focus to the neighbour (splits spec, P3).

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Axis, GroupId, Member, Side, View};

use super::splits::{centre, close, current, focused, last, shown_in};
use super::{ab_focused, at, focus_tree, keys, open, select, tall_notes};
use crate::shell::AppShell;
use crate::shell::docks::MoveBacklinksToOtherDock;
use crate::shell::split::{SplitDown, SplitRight};

fn root_axis(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Axis {
    window
        .update(cx, |shell, _, _| match shell.layout.center() {
            Member::Split { axis, .. } => *axis,
            Member::Group(_) => panic!("the centre is split"),
        })
        .unwrap()
}

fn editor_focused_in(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    group: GroupId,
) -> bool {
    let editor = window
        .update(cx, |shell, _, _| shell.editor_of(group).cloned())
        .unwrap()
        .expect("the group shows a note");
    focused(window, cx, &editor)
}

fn tree_focused(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .unwrap()
}

fn left_groups(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<GroupId> {
    window
        .update(cx, |shell, _, _| shell.layout.dock_groups(Side::Left))
        .unwrap()
}

/// `group` becomes `last_center`, its editor focused.
fn enter(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, group: GroupId) {
    window
        .update(cx, |shell, window, cx| {
            shell.activate_group(group, window, cx);
            if let Some(focus) = shell.land_focus(cx) {
                window.focus(&focus, cx);
            }
        })
        .unwrap();
    cx.run_until_parked();
}

/// One note in four groups: upper left, upper right, lower left, lower right.
fn grid(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> [GroupId; 4] {
    let upper_left = last(window, cx);
    cx.dispatch_action(SplitRight);
    cx.run_until_parked();
    let upper_right = last(window, cx);
    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
    let lower_right = last(window, cx);
    enter(window, cx, upper_left);
    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
    let lower_left = last(window, cx);
    [upper_left, upper_right, lower_left, lower_right]
}

#[gpui::test]
fn cmd_k_left_splits_left(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let first = last(&window, &mut cx);

    keys(&mut cx, "cmd-k left");

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0], first);
    assert_eq!(groups[1], first);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(root_axis(&window, &mut cx), Axis::Horizontal);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert!(editor_focused_in(&window, &mut cx, groups[0]));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_right_splits_right(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let first = last(&window, &mut cx);

    keys(&mut cx, "cmd-k right");

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], first);
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(root_axis(&window, &mut cx), Axis::Horizontal);
    assert!(editor_focused_in(&window, &mut cx, groups[1]));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_up_splits_up(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let first = last(&window, &mut cx);

    keys(&mut cx, "cmd-k up");

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[1], first);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(root_axis(&window, &mut cx), Axis::Vertical);
    assert!(editor_focused_in(&window, &mut cx, groups[0]));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_down_splits_down(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let first = last(&window, &mut cx);

    keys(&mut cx, "cmd-k down");

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0], first);
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(root_axis(&window, &mut cx), Axis::Vertical);
    assert!(editor_focused_in(&window, &mut cx, groups[1]));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_left_leaves_the_caret(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 2..2);

    keys(&mut cx, "cmd-k left");

    for group in centre(&window, &mut cx) {
        let editor = window
            .update(&mut cx, |shell, _, _| shell.editor_of(group).cloned())
            .unwrap()
            .expect("the group shows a note");
        assert_eq!(editor.read_with(&cx, |editor, _| editor.cursor()), at(2));
    }
    assert_eq!(current(&window, &mut cx), Some(a));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_left_focuses_the_left_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let [upper_left, upper_right, ..] = grid(&window, &mut cx);
    enter(&window, &mut cx, upper_right);

    keys(&mut cx, "cmd-k cmd-left");

    assert_eq!(last(&window, &mut cx), upper_left);
    assert!(editor_focused_in(&window, &mut cx, upper_left));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_right_focuses_the_right_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let [upper_left, upper_right, ..] = grid(&window, &mut cx);
    enter(&window, &mut cx, upper_left);

    keys(&mut cx, "cmd-k cmd-right");

    assert_eq!(last(&window, &mut cx), upper_right);
    assert!(editor_focused_in(&window, &mut cx, upper_right));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_up_focuses_the_upper_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let [upper_left, _, lower_left, _] = grid(&window, &mut cx);
    enter(&window, &mut cx, lower_left);

    keys(&mut cx, "cmd-k cmd-up");

    assert_eq!(last(&window, &mut cx), upper_left);
    assert!(editor_focused_in(&window, &mut cx, upper_left));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_down_focuses_the_lower_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let [_, upper_right, _, lower_right] = grid(&window, &mut cx);
    enter(&window, &mut cx, upper_right);

    keys(&mut cx, "cmd-k cmd-down");

    assert_eq!(last(&window, &mut cx), lower_right);
    assert!(editor_focused_in(&window, &mut cx, lower_right));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_left_from_the_left_edge_focuses_the_sidebar(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let first = last(&window, &mut cx);

    keys(&mut cx, "cmd-k cmd-left");

    assert!(tree_focused(&window, &mut cx));
    assert_eq!(last(&window, &mut cx), first);
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_cmd_right_from_the_sidebar_returns_to_last_center(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-k right");
    let right = last(&window, &mut cx);
    focus_tree(&window, &mut cx);

    keys(&mut cx, "cmd-k cmd-right");

    assert_eq!(last(&window, &mut cx), right);
    assert!(editor_focused_in(&window, &mut cx, right));
    close(&mut cx);
}

/// A half-way default would land lower left both times: the boundary is half-open.
#[gpui::test]
fn the_probe_follows_the_caret(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    open(&window, &mut cx, a);
    let upper_left = last(&window, &mut cx);
    keys(&mut cx, "cmd-k right");
    let right = last(&window, &mut cx);
    enter(&window, &mut cx, upper_left);
    keys(&mut cx, "cmd-k down");
    let lower_left = last(&window, &mut cx);
    assert_eq!(centre(&window, &mut cx).len(), 3);
    enter(&window, &mut cx, right);
    select(&window, &mut cx, 0..0);

    keys(&mut cx, "cmd-k cmd-left");

    assert_eq!(last(&window, &mut cx), upper_left);
    enter(&window, &mut cx, right);
    select(&window, &mut cx, 1000..1000);

    keys(&mut cx, "cmd-k cmd-left");

    assert_eq!(last(&window, &mut cx), lower_left);
    assert!(editor_focused_in(&window, &mut cx, lower_left));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_left_from_the_sidebar_with_one_view_does_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);

    keys(&mut cx, "cmd-k left");

    assert_eq!(left_groups(&window, &mut cx).len(), 1);
    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert!(tree_focused(&window, &mut cx));
    close(&mut cx);
}

#[gpui::test]
fn cmd_k_down_from_the_sidebar_splits_files_off(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    window
        .update(&mut cx, |shell, window, cx| {
            shell.reveal(View::Files, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    focus_tree(&window, &mut cx);

    keys(&mut cx, "cmd-k down");

    let groups = left_groups(&window, &mut cx);
    let held = window
        .update(&mut cx, |shell, _, _| {
            groups
                .iter()
                .map(|&group| shell.group(group).views().map(|(_, view)| view).collect())
                .collect::<Vec<Vec<_>>>()
        })
        .unwrap();
    assert_eq!(held, [vec![View::Backlinks], vec![View::Files]]);
    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert!(tree_focused(&window, &mut cx));
    close(&mut cx);
}
