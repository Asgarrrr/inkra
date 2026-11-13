//! A tab dragged between groups, and a view dragged into the centre (splits spec, P4).

use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Layout, Side, SplitDirection, ViewId};

use super::dock_menu::{default_docks, docks};
use super::dragging::drag;
use super::edge_drops::hold;
use super::restore_lands::{launch, vault};
use super::session_writes::wait;
use super::splits::{
    centre, check_layout, close, editor_in, focused, last, shown_in, split, tabs_in,
};
use super::tab_bar::click;
use super::{AppShell, ab_focused, four_notes, keys, open};
use crate::open::Open;
use crate::shell::FILES;
use crate::shell::GoBack;
use crate::shell::docks::SplitView;
use crate::shell::session::THROTTLE;

fn shown_view(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    k: usize,
) -> Option<ViewId> {
    let group = centre(window, cx)[k];
    window
        .update(cx, |shell, _, _| shell.group(group).shown.view())
        .unwrap()
}

fn files_focused(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .unwrap()
}

#[gpui::test]
fn dragging_a_tab_onto_another_group_moves_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);

    drag(&mut cx, "tab-0", "tab-0-1");

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(b)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(a), Some(b)]);
    assert_eq!(last(&window, &mut cx), centre(&window, &mut cx)[1]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    let editor = editor_in(&window, &mut cx, 1);
    assert!(focused(&window, &mut cx, &editor));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_moved_tab_keeps_its_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    split(&mut cx);

    drag(&mut cx, "tab-0", "tab-0-1");
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    cx.dispatch_action(GoBack);
    cx.run_until_parked();

    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(a), Some(c)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn moving_a_groups_last_tab_removes_the_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);

    drag(&mut cx, "tab-0-1", "tab-0");

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 1);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(b), Some(a), Some(b)]);
    let editor = editor_in(&window, &mut cx, 0);
    assert!(focused(&window, &mut cx, &editor));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

/// `a` open in group 0, then the Files icon dropped on group 0's strip.
fn files_in_the_centre(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<AppShell>,
    VisualTestContext,
    inkra_vault::EntryId,
    inkra_vault::EntryId,
) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    drag(&mut cx, "dock-icon-files", "group-strip");
    (window, cx, a, b)
}

#[gpui::test]
fn a_view_dragged_onto_a_centre_strip_shows_there(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = files_in_the_centre(cx);

    let left = window
        .update(&mut cx, |shell, _, _| shell.layout.dock_groups(Side::Left))
        .unwrap();
    assert!(left.is_empty());
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), None]);
    assert_eq!(shown_view(&window, &mut cx, 0), Some(FILES));
    assert!(cx.debug_bounds("panel-header-files").is_some());
    assert!(files_focused(&window, &mut cx));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_note_tab_dragged_over_a_dock_drops_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);

    drag(&mut cx, "tab-0", "dock-left");

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(b)]);
    assert_eq!(docks(&window, &mut cx), default_docks());
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn closing_a_view_tab_in_the_centre_returns_it_to_its_dock(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = files_in_the_centre(cx);

    click(&mut cx, "tab-close-1");

    assert_eq!(docks(&window, &mut cx), default_docks());
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

fn layout(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Layout {
    window
        .update(cx, |shell, _, _| shell.layout.clone())
        .unwrap()
}

#[gpui::test]
fn closing_a_centre_files_tab_redocks_it_and_reopens_nothing(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = files_in_the_centre(cx);

    click(&mut cx, "tab-close-1");

    assert_eq!(docks(&window, &mut cx), default_docks());
    let group = centre(&window, &mut cx)[0];
    let has_closed = window
        .update(&mut cx, |shell, _, _| shell.group(group).tabs.has_closed())
        .unwrap();
    assert!(!has_closed);
    let before = layout(&window, &mut cx);
    keys(&mut cx, "cmd-shift-t");
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(layout(&window, &mut cx), before);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn opening_a_note_over_a_view_tab_opens_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut cx, a, b) = files_in_the_centre(cx);

    window
        .update(&mut cx, |shell, _, cx| shell.open(b, None, Open::Here, cx))
        .unwrap();
    cx.run_until_parked();

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), None, Some(b)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(b));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_view_in_the_centre_restores_to_its_default_dock(cx: &mut TestAppContext) {
    let fs = vault();
    let (window, mut visual) = launch(cx, &fs);
    visual.run_until_parked();
    drag(&mut visual, "dock-icon-files", "group-strip");
    assert_eq!(shown_view(&window, &mut visual, 0), Some(FILES));
    wait(&mut visual, THROTTLE);
    close(&mut visual);

    let (window, mut visual) = launch(cx, &fs);
    visual.run_until_parked();

    assert_eq!(docks(&window, &mut visual), default_docks());
    assert_eq!(shown_view(&window, &mut visual, 0), None);
    check_layout(&window, &mut visual);
    close(&mut visual);
}

#[gpui::test]
fn split_right_on_a_centre_view_moves_the_view(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = files_in_the_centre(cx);

    split(&mut cx);

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [None]);
    assert_eq!(shown_view(&window, &mut cx, 1), Some(FILES));
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert!(files_focused(&window, &mut cx));
    split(&mut cx);
    assert_eq!(
        centre(&window, &mut cx),
        groups,
        "a lone view splits nothing"
    );
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_tab_closed_while_held_drops_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    let middle = cx.debug_bounds("note-column").expect("drawn").center();
    hold(&mut cx, "tab-0-1", middle);

    keys(&mut cx, "cmd-w");
    cx.simulate_mouse_move(middle, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(middle, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(b)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn split_view_of_an_unknown_view_changes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let before = (layout(&window, &mut cx), docks(&window, &mut cx));

    cx.dispatch_action(SplitView {
        view: ViewId::new("nope"),
        direction: SplitDirection::Right,
    });
    cx.run_until_parked();

    assert_eq!((layout(&window, &mut cx), docks(&window, &mut cx)), before);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}
