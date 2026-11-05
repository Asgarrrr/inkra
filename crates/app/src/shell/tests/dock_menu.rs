//! The dock menu: a right-click on a header or an icon, a choice by mouse (docks spec, Context menu).

use gpui::{Focusable, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Docks, Side, View};

use super::tab_bar::{click, press};
use super::{ab_focused, editor_focused, focus_tree, keys, open};
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

/// The highlighted row, read from the draw: the background itself reaches no test.
fn highlighted(cx: &mut VisualTestContext) -> Option<usize> {
    [
        "context-menu-item-0-selected",
        "context-menu-item-1-selected",
    ]
    .into_iter()
    .position(|selector| drawn(cx, selector))
}

pub(super) fn docks(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Docks {
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

#[gpui::test]
fn down_then_enter_moves_the_view(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    keys(&mut cx, "down enter");

    assert_eq!(docks(&window, &mut cx).side_of(View::Files), Side::Right);
    assert!(!drawn(&mut cx, "context-menu"));
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn up_then_enter_hides_the_dock(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    keys(&mut cx, "up enter");

    assert!(!docks(&window, &mut cx).is_visible(View::Files));
}

#[gpui::test]
fn down_wraps_to_the_first_entry(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    assert_eq!(highlighted(&mut cx), None);
    for expected in [0, 1, 0] {
        keys(&mut cx, "down");
        assert_eq!(highlighted(&mut cx), Some(expected));
    }
    keys(&mut cx, "enter");

    assert_eq!(docks(&window, &mut cx).side_of(View::Files), Side::Right);
}

#[gpui::test]
fn up_wraps_to_the_last_entry(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    for expected in [1, 0, 1] {
        keys(&mut cx, "up");
        assert_eq!(highlighted(&mut cx), Some(expected));
    }
    keys(&mut cx, "enter");

    assert!(!docks(&window, &mut cx).is_visible(View::Files));
}

#[gpui::test]
fn enter_with_nothing_selected_does_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    keys(&mut cx, "enter");

    assert!(drawn(&mut cx, "context-menu"));
    assert!(menu_focused(&window, &mut cx));
    assert_eq!(docks(&window, &mut cx), Docks::default());
}

#[gpui::test]
fn escape_closes_the_menu_and_gives_focus_back(cx: &mut TestAppContext) {
    let (window, mut cx) = files_menu(cx);

    keys(&mut cx, "escape");

    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(docks(&window, &mut cx), Docks::default());
    assert!(editor_focused(&window, &mut cx));
}

/// Focus is on the tree, not the editor: the shell's fallback is the editor, so only the menu's own restore lands on the tree.
#[gpui::test]
fn a_picker_over_the_menu_keeps_the_menus_focus_return(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    focus_tree(&window, &mut cx);
    press(&mut cx, "panel-header-files", MouseButton::Right);
    assert!(menu_focused(&window, &mut cx));

    keys(&mut cx, "cmd-p");
    let picker_replaced_menu = window
        .update(&mut cx, |shell, _, _| {
            shell.modal.is_some() && shell.modal_menu().is_none()
        })
        .unwrap();
    assert!(picker_replaced_menu);

    keys(&mut cx, "escape");

    assert!(!drawn(&mut cx, "context-menu"));
    let tree_focused = window
        .update(&mut cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .unwrap();
    assert!(tree_focused);
}
