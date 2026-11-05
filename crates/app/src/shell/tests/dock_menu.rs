//! The dock menu: a right-click on a header or an icon, a choice by mouse (docks spec, Context menu).

use gpui::{Focusable, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Docks, Side, View};

use super::tab_bar::{click, press};
use super::{ab_focused, editor_focused, keys, open};
use crate::shell::AppShell;
use crate::shell::docks::MoveBacklinksToOtherDock;

fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn menu_focused(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell
                .modal_menu()
                .is_some_and(|menu| menu.focus_handle(cx).is_focused(window))
        })
        .unwrap()
}

#[gpui::test]
fn a_right_click_on_the_files_header_opens_its_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    press(&mut cx, "panel-header-files", MouseButton::Right);

    for shown in ["context-menu", "context-menu-item-0", "context-menu-item-1"] {
        assert!(drawn(&mut cx, shown), "{shown}");
    }
    assert!(!drawn(&mut cx, "context-menu-item-2"));
    assert!(menu_focused(&window, &mut cx));
}

fn docks(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Docks {
    window
        .update(cx, |shell, _, _| shell.panes().docks.clone())
        .unwrap()
}

/// The Files header's menu, open over a focused editor.
fn files_menu(cx: &mut TestAppContext) -> (WindowHandle<AppShell>, VisualTestContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    press(&mut cx, "panel-header-files", MouseButton::Right);
    assert!(menu_focused(&window, &mut cx));
    (window, cx)
}

#[gpui::test]
fn choosing_move_moves_the_view_and_gives_focus_back(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    click(&mut cx, "context-menu-item-0");

    assert_eq!(docks(&window, &mut cx).side_of(View::Files), Side::Right);
    assert!(!drawn(&mut cx, "context-menu"));
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn choosing_hide_hides_the_dock(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    click(&mut cx, "context-menu-item-1");

    assert!(!docks(&window, &mut cx).is_visible(View::Files));
    assert!(editor_focused(&window, &mut cx));
}

fn press_outside(cx: &mut VisualTestContext, button: MouseButton) {
    let at = cx.debug_bounds("note-column").expect("drawn").center();
    cx.simulate_mouse_down(at, button, Modifiers::none());
    cx.simulate_mouse_up(at, button, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_click_outside_closes_the_menu(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    press_outside(&mut cx, MouseButton::Left);

    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(docks(&window, &mut cx), Docks::default());
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn a_right_click_outside_closes_the_menu(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    press_outside(&mut cx, MouseButton::Right);

    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(docks(&window, &mut cx), Docks::default());
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn a_right_click_on_an_icon_opens_that_views_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();

    press(&mut cx, "dock-icon-files", MouseButton::Right);
    click(&mut cx, "context-menu-item-0");

    assert_eq!(docks(&window, &mut cx).side_of(View::Files), Side::Right);
}

#[gpui::test]
fn a_dock_toggle_closes_an_open_menu(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    keys(&mut cx, "cmd-\\");

    assert!(!drawn(&mut cx, "context-menu"));
    assert!(editor_focused(&window, &mut cx));
}
