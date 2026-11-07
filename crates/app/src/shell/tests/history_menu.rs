//! The history menu: a right-click on an arrow lists the active tab's history; a row walks there in one step.

use gpui::{MouseButton, TestAppContext};
use inkra_tabs::Way;

use super::dock_menu::drawn;
use super::splits::{self, centre, last, shown_in, split};
use super::tab_bar::{click, press};
use super::{ab_focused, editor_focused, four_notes, keys, open, showing, start_open};
use crate::shell::nav::GoToStop;

#[gpui::test]
fn a_row_walks_the_history_in_one_step(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }

    press(&mut cx, "nav-back", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-current-0"));
    assert!(drawn(&mut cx, "context-menu-item-2"));
    assert!(!drawn(&mut cx, "context-menu-item-3"));

    click(&mut cx, "context-menu-item-2");
    assert_eq!(showing(&window, &mut cx), Some(a));
    assert!(drawn(&mut cx, "nav-back-disabled"));

    keys(&mut cx, "cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(b));
    keys(&mut cx, "cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(c));
}

#[gpui::test]
fn the_menu_lists_forward_far_to_near_then_the_current_row_then_back(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    for note in [a, b, c, d] {
        open(&window, &mut cx, note);
    }
    keys(&mut cx, "cmd-[ cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(b));

    press(&mut cx, "nav-forward", MouseButton::Right);
    for item in [
        "context-menu-item-0",
        "context-menu-item-1",
        "context-menu-item-3",
    ] {
        assert!(drawn(&mut cx, item));
    }
    assert!(drawn(&mut cx, "context-menu-item-current-2"));
    assert!(!drawn(&mut cx, "context-menu-item-4"));

    click(&mut cx, "context-menu-item-0");
    assert_eq!(showing(&window, &mut cx), Some(d));
    for note in [c, b, a] {
        keys(&mut cx, "cmd-[");
        assert_eq!(showing(&window, &mut cx), Some(note));
    }
}

#[gpui::test]
fn the_current_row_closes_the_menu_and_moves_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    for note in [a, b, c, d] {
        open(&window, &mut cx, note);
    }
    keys(&mut cx, "cmd-[ cmd-[");

    press(&mut cx, "nav-forward", MouseButton::Right);
    click(&mut cx, "context-menu-item-current-2");

    assert!(!drawn(&mut cx, "context-menu"));
    assert_eq!(showing(&window, &mut cx), Some(b));
    keys(&mut cx, "cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(c));
}

#[gpui::test]
fn a_dimmed_arrow_opens_the_menu_when_the_other_way_reaches(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);

    press(&mut cx, "nav-forward-disabled", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-current-0"));
    assert!(drawn(&mut cx, "context-menu-item-1"));
    assert!(!drawn(&mut cx, "context-menu-item-2"));

    click(&mut cx, "context-menu-item-1");
    assert_eq!(showing(&window, &mut cx), Some(a));
}

#[gpui::test]
fn a_right_click_with_no_history_opens_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, ..]) = four_notes(cx);
    open(&window, &mut cx, a);

    for arrow in ["nav-back-disabled", "nav-forward-disabled"] {
        press(&mut cx, arrow, MouseButton::Right);

        assert!(!drawn(&mut cx, "context-menu"));
        assert!(editor_focused(&window, &mut cx));
    }
}

#[gpui::test]
fn the_menu_holds_at_most_seven_stops_each_way(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    for note in [a, b, a, b, a, b, a, b, a] {
        open(&window, &mut cx, note);
    }

    press(&mut cx, "nav-back", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-current-0"));
    assert!(drawn(&mut cx, "context-menu-item-7"));
    assert!(!drawn(&mut cx, "context-menu-item-8"));

    keys(&mut cx, "escape");
    for _ in 0..8 {
        keys(&mut cx, "cmd-[");
    }
    press(&mut cx, "nav-forward", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-6"));
    assert!(drawn(&mut cx, "context-menu-item-current-7"));
    assert!(!drawn(&mut cx, "context-menu-item-8"));
}

#[gpui::test]
fn a_row_left_stale_by_a_landing_walks_nothing(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    let gate = fs.block_load("/vault/d.md");
    start_open(&window, &mut cx, d);

    press(&mut cx, "nav-back", MouseButton::Right);
    gate.release();
    cx.run_until_parked();
    assert_eq!(showing(&window, &mut cx), Some(d));
    assert!(drawn(&mut cx, "context-menu"));

    click(&mut cx, "context-menu-item-1");
    assert_eq!(showing(&window, &mut cx), Some(d));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(c));
}

#[gpui::test]
fn a_row_walks_the_group_its_menu_was_opened_on(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    split(&mut cx);
    open(&window, &mut cx, c);
    open(&window, &mut cx, d);

    let groups = centre(&window, &mut cx);
    let stop = window
        .update(&mut cx, |shell, _, _| {
            let group = shell.group(groups[1]);
            group.tabs.stops(Way::Back, group.shown.file()).nth(1)
        })
        .unwrap()
        .expect("group 1 has been on b");
    keys(&mut cx, "cmd-k cmd-left");
    assert_eq!(last(&window, &mut cx), groups[0]);

    cx.dispatch_action(GoToStop {
        group: groups[1],
        stop,
    });
    cx.run_until_parked();
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    assert_eq!(shown_in(&window, &mut cx, 0), Some(b));
    assert_eq!(last(&window, &mut cx), groups[0]);
    splits::close(&mut cx);
}

#[gpui::test]
fn a_row_for_a_closed_group_walks_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    split(&mut cx);
    open(&window, &mut cx, a);
    let group = centre(&window, &mut cx)[1];
    let stop = window
        .update(&mut cx, |shell, _, _| {
            let group = shell.group(group);
            group.tabs.stops(Way::Back, group.shown.file()).next()
        })
        .unwrap()
        .expect("group 1 has been on b");

    keys(&mut cx, "cmd-w");
    assert_eq!(centre(&window, &mut cx).len(), 1);

    cx.dispatch_action(GoToStop { group, stop });
    cx.run_until_parked();
    assert_eq!(showing(&window, &mut cx), Some(b));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    splits::close(&mut cx);
}
