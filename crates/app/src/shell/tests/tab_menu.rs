//! The tab menu: a right-click on a tab activates its group and the tab, then lists Split Right and Close.

use gpui::{MouseButton, TestAppContext};

use super::dock_menu::{drawn, menu_focused};
use super::splits::{centre, check_layout, close, last, shown_in, split, tabs_in};
use super::tab_bar::{click, press};
use super::{ab_focused, editor_focused, keys, open};

#[gpui::test]
fn a_right_click_on_a_tab_opens_its_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    press(&mut cx, "tab-0", MouseButton::Right);

    for shown in ["context-menu", "context-menu-item-0", "context-menu-item-1"] {
        assert!(drawn(&mut cx, shown), "{shown}");
    }
    assert!(!drawn(&mut cx, "context-menu-item-2"));
    assert!(menu_focused(&window, &mut cx));
}

#[gpui::test]
fn a_right_click_on_another_groups_tab_activates_that_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    let groups = centre(&window, &mut cx);
    assert_eq!(last(&window, &mut cx), groups[1]);

    press(&mut cx, "tab-0", MouseButton::Right);

    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert!(menu_focused(&window, &mut cx));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn choosing_split_right_splits_that_tabs_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    open(&window, &mut cx, b);
    let groups = centre(&window, &mut cx);

    press(&mut cx, "tab-0", MouseButton::Right);
    click(&mut cx, "context-menu-item-0");

    let split = centre(&window, &mut cx);
    assert_eq!(split.len(), 3);
    assert_eq!([split[0], split[2]], [groups[0], groups[1]]);
    assert_eq!(last(&window, &mut cx), split[1]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    assert_eq!(shown_in(&window, &mut cx, 2), Some(b));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn down_then_enter_chooses_close(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);

    press(&mut cx, "tab-0", MouseButton::Right);
    keys(&mut cx, "down down enter");

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(b)]);
    assert!(!drawn(&mut cx, "context-menu"));
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn escape_closes_the_tab_menu(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    press(&mut cx, "tab-0", MouseButton::Right);

    keys(&mut cx, "escape");

    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(b)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert!(editor_focused(&window, &mut cx));
}
