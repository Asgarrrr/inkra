//! A dock view split off into its own group, kept across a relaunch (splits spec, S8).

use gpui::{MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{GroupId, Side};

use super::dock_menu::{docks, drawn};
use super::restore_lands::{launch, vault};
use super::session_writes::wait;
use super::splits::{centre, close};
use super::tab_bar::{click, press};
use super::{ab_focused, open};
use crate::backlinks::BACKLINKS;
use crate::backlinks::MoveBacklinksToOtherDock;
use crate::shell::AppShell;
use crate::shell::FILES;
use crate::shell::session::THROTTLE;
use crate::shell::split::SplitRight;
use crate::theme::TITLE_BAR_HEIGHT;

fn left_groups(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<GroupId> {
    window
        .update(cx, |shell, _, _| shell.layout.dock_groups(Side::Left))
        .unwrap()
}

/// Both views in the left dock, then Backlinks split down off Files through its menu.
fn split_backlinks_down(cx: &mut VisualTestContext) {
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();
    press(cx, "dock-icon-backlinks", MouseButton::Right);
    click(cx, "context-menu-item-2");
}

fn assert_split_left(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    let groups = left_groups(window, cx);
    assert_eq!(groups.len(), 2);
    let held = window
        .update(cx, |shell, _, _| {
            groups
                .iter()
                .map(|&group| shell.group(group).views().map(|(_, view)| view).collect())
                .collect::<Vec<Vec<_>>>()
        })
        .unwrap();
    assert_eq!(held, [vec![FILES], vec![BACKLINKS]]);
    let column = cx.debug_bounds("dock-left").expect("drawn");
    let bar = cx.debug_bounds("dock-bar-left-1").expect("drawn");
    assert!(column.contains(&bar.center()), "{bar:?} in {column:?}");
    assert!(bar.top() > TITLE_BAR_HEIGHT, "{bar:?}");
}

#[gpui::test]
fn split_down_on_a_dock_view_gives_two_dock_groups(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    split_backlinks_down(&mut cx);

    assert_split_left(&window, &mut cx);
    assert!(drawn(&mut cx, "dock-left-group-1"));
    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(
        docks(&window, &mut cx)[1],
        (Vec::new(), None, true),
        "the right dock emptied"
    );
}

#[gpui::test]
fn the_dock_menu_offers_no_split_for_a_lone_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    press(&mut cx, "dock-icon-files", MouseButton::Right);

    assert!(drawn(&mut cx, "context-menu-item-1"));
    assert!(!drawn(&mut cx, "context-menu-item-2"));
    cx.simulate_keystrokes("escape");
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();
    press(&mut cx, "dock-icon-files", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-3"));
    assert_eq!(left_groups(&window, &mut cx).len(), 1);
}

#[gpui::test]
fn a_split_dock_survives_a_relaunch(cx: &mut TestAppContext) {
    let fs = vault();
    let (_window, mut visual) = launch(cx, &fs);
    visual.run_until_parked();
    split_backlinks_down(&mut visual);
    wait(&mut visual, THROTTLE);
    close(&mut visual);

    let (window, mut visual) = launch(cx, &fs);
    visual.run_until_parked();

    assert_split_left(&window, &mut visual);
    close(&mut visual);
}

/// A dock group's only tab never takes a centre group's way out (L6 is the centre's).
#[gpui::test]
fn close_tab_on_a_dock_group_leaves_the_centre_alone(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(SplitRight);
    cx.run_until_parked();

    window
        .update(&mut cx, |shell, window, cx| {
            let group = shell.layout.dock_groups(Side::Left)[0];
            let tab = shell.group(group).tabs.active();
            shell.close_tab(group, tab, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(centre(&window, &mut cx).len(), 2);
    close(&mut cx);
}
