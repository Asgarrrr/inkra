use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Entity, Focusable, Modifiers, MouseButton, Subscription, TestAppContext, VisualTestContext,
};

use super::letters::{DELAY, Letters};
use super::{Dismissed, Picker};
use crate::keymap::key_bindings;

const ROWS: &[&str] = &["# fruit", "apple", "apricot", "# veg", "beet"];

fn open(
    cx: &mut TestAppContext,
    rows: &'static [&'static str],
) -> (Entity<Picker<Letters>>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let (picker, visual) =
        cx.add_window_view(move |window, cx| Picker::new(Letters::new(rows), window, cx));
    let mut visual = visual.clone();
    visual.update(|window, cx| window.focus(&picker.focus_handle(cx), cx));
    settle(&mut visual);
    (picker, visual)
}

/// Lets `Letters` answer the query typed last.
fn settle(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    visual.executor().advance_clock(DELAY);
    visual.run_until_parked();
}

fn selected(picker: &Entity<Picker<Letters>>, cx: &mut VisualTestContext) -> Option<usize> {
    picker.read_with(cx, |picker, _| picker.selected())
}

fn confirmed(picker: &Entity<Picker<Letters>>, cx: &mut VisualTestContext) -> Vec<&'static str> {
    picker.read_with(cx, |picker, _| picker.delegate.confirmed.borrow().clone())
}

fn watch_dismissed(
    picker: &Entity<Picker<Letters>>,
    cx: &mut VisualTestContext,
) -> (Rc<Cell<usize>>, Subscription) {
    let seen = Rc::new(Cell::new(0));
    let guard = cx.update(|_, cx| {
        let seen = Rc::clone(&seen);
        cx.subscribe(picker, move |_, _: &Dismissed, _| seen.set(seen.get() + 1))
    });
    (seen, guard)
}

fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual.debug_bounds(selector);
    assert!(bounds.is_some(), "{selector} is drawn");
    if let Some(bounds) = bounds {
        visual.simulate_click(bounds.center(), Modifiers::none());
    }
}

#[gpui::test]
fn the_first_selectable_row_is_selected(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    assert_eq!(selected(&picker, &mut visual), Some(1), "row 0 is a header");
}

#[gpui::test]
fn down_and_up_skip_the_headers(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);

    visual.simulate_keystrokes("down down");
    assert_eq!(
        selected(&picker, &mut visual),
        Some(4),
        "apricot, then over `# veg` to beet"
    );

    visual.simulate_keystrokes("up");
    assert_eq!(selected(&picker, &mut visual), Some(2));
}

#[gpui::test]
fn up_and_down_stop_at_the_ends(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);

    visual.simulate_keystrokes("up");
    assert_eq!(
        selected(&picker, &mut visual),
        Some(1),
        "only a header is above"
    );

    visual.simulate_keystrokes("down down down");
    assert_eq!(selected(&picker, &mut visual), Some(4));
}

#[gpui::test]
fn typing_runs_the_query_and_selects_its_first_row(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    visual.simulate_keystrokes("down");

    visual.simulate_keystrokes("e");
    settle(&mut visual);

    assert_eq!(
        picker.read_with(&visual, |picker, _| picker.query().to_owned()),
        "e"
    );
    assert_eq!(
        selected(&picker, &mut visual),
        Some(0),
        "apple, # veg, beet: the selection starts over"
    );
}

#[gpui::test]
fn a_new_query_drops_the_one_before(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);

    visual.simulate_keystrokes("a p");
    settle(&mut visual);

    let applied = picker.read_with(&visual, |picker, _| {
        picker.delegate.applied.borrow().clone()
    });
    assert_eq!(applied, ["", "ap"], "`a` never reached the picker");
}

#[gpui::test]
fn enter_confirms_the_selected_row_and_dismisses(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);

    visual.simulate_keystrokes("down enter");

    assert_eq!(confirmed(&picker, &mut visual), ["apricot"]);
    assert_eq!(dismissed.get(), 1);
}

#[gpui::test]
fn enter_with_no_row_does_nothing(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);
    visual.simulate_keystrokes("z");
    settle(&mut visual);

    visual.simulate_keystrokes("enter");

    assert_eq!(confirmed(&picker, &mut visual), Vec::<&str>::new());
    assert_eq!(dismissed.get(), 0);
}

#[gpui::test]
fn escape_dismisses_without_confirming(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);

    visual.simulate_keystrokes("escape");

    assert_eq!(confirmed(&picker, &mut visual), Vec::<&str>::new());
    assert_eq!(dismissed.get(), 1);
}

#[gpui::test]
fn a_click_confirms_the_row_it_lands_on(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);

    click(&mut visual, "picker-row-4");

    assert_eq!(confirmed(&picker, &mut visual), ["beet"]);
    assert_eq!(dismissed.get(), 1);
}

#[gpui::test]
fn a_click_on_a_header_does_nothing(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);

    click(&mut visual, "picker-row-3");

    assert_eq!(confirmed(&picker, &mut visual), Vec::<&str>::new());
    assert_eq!(dismissed.get(), 0);
    assert_eq!(selected(&picker, &mut visual), Some(1));
}

#[gpui::test]
fn an_update_past_the_last_selectable_row_selects_the_one_before(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, &["a", "# h"]);

    picker.update(&mut visual, |picker, cx| picker.matches_updated(1, cx));

    assert_eq!(selected(&picker, &mut visual), Some(0));
}

#[gpui::test]
fn losing_focus_dismisses(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);

    // The picker dismisses on blur only in an active window, as Zed's.
    visual.update(|window, _| window.activate_window());
    // Test activation lands asynchronously (platform/test/platform.rs:294-313), and a blur counts only from a frame drawn active.
    visual.run_until_parked();
    visual.update(|window, cx| {
        let elsewhere = cx.focus_handle();
        window.focus(&elsewhere, cx);
    });
    visual.run_until_parked();

    assert_eq!(dismissed.get(), 1);
}

#[gpui::test]
fn switching_to_another_app_keeps_it_open(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();

    visual.deactivate_window();

    assert_eq!(dismissed.get(), 0);
}

#[gpui::test]
fn moving_past_the_last_visible_row_scrolls_it_into_view(cx: &mut TestAppContext) {
    const MANY: &[&str] = &[
        "r00", "r01", "r02", "r03", "r04", "r05", "r06", "r07", "r08", "r09", "r10", "r11", "r12",
        "r13", "r14",
    ];
    let (picker, mut visual) = open(cx, MANY);
    let last_visible = visual.debug_bounds("picker-row-9");
    assert!(last_visible.is_some(), "ten rows fit");
    assert!(
        visual.debug_bounds("picker-row-12").is_none(),
        "row 12 starts below the list"
    );

    visual.simulate_keystrokes("down down down down down down down down down down down down");
    assert_eq!(selected(&picker, &mut visual), Some(12));

    let row = visual.debug_bounds("picker-row-12");
    assert!(
        row.zip(last_visible)
            .is_some_and(|(row, last)| row.bottom() <= last.bottom()),
        "row 12 {row:?} is drawn and ends within the list, above {last_visible:?}"
    );
}

#[gpui::test]
fn enter_while_a_query_is_pending_confirms_its_first_row(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let (dismissed, _guard) = watch_dismissed(&picker, &mut visual);
    visual.simulate_keystrokes("down");

    visual.simulate_keystrokes("b enter");
    assert_eq!(confirmed(&picker, &mut visual), Vec::<&str>::new());
    settle(&mut visual);

    assert_eq!(confirmed(&picker, &mut visual), ["beet"], "not apricot");
    assert_eq!(dismissed.get(), 1);
}

/// As Zed's rows: a press alone does nothing, so a press can still be dragged off the row.
#[gpui::test]
fn a_row_confirms_on_release_not_on_press(cx: &mut TestAppContext) {
    let (picker, mut visual) = open(cx, ROWS);
    let row = visual.debug_bounds("picker-row-4");
    assert!(row.is_some(), "row 4 is drawn");
    let at = row.map(|row| row.center()).unwrap_or_default();

    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    assert_eq!(confirmed(&picker, &mut visual), Vec::<&str>::new());

    visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    assert_eq!(confirmed(&picker, &mut visual), ["beet"]);
}
