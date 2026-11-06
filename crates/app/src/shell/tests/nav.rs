use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};

use super::tab_bar::click;
use super::{ab_focused, editor_focused, open, showing, splits, tab_notes};
use crate::shell::split::SplitDown;

#[gpui::test]
fn the_arrows_walk_the_tabs_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);

    click(&mut cx, "nav-back");
    assert_eq!(showing(&window, &mut cx), Some(a));

    click(&mut cx, "nav-forward");
    assert_eq!(showing(&window, &mut cx), Some(b));
}

#[gpui::test]
fn a_dimmed_arrow_changes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let before = (showing(&window, &mut cx), tab_notes(&window, &mut cx));
    assert!(editor_focused(&window, &mut cx));

    // The view lights an arrow from `Tabs`: a disabled `IconButton` draws as `{id}-disabled`.
    for arrow in ["nav-back-disabled", "nav-forward-disabled"] {
        click(&mut cx, arrow);
    }
    assert!(cx.debug_bounds("nav-back").is_none());
    assert!(cx.debug_bounds("nav-forward").is_none());

    assert_eq!(
        (showing(&window, &mut cx), tab_notes(&window, &mut cx)),
        before
    );
    assert!(
        editor_focused(&window, &mut cx),
        "a dimmed arrow keeps focus"
    );
}

#[gpui::test]
fn an_arrow_is_lit_only_when_its_walk_would_arrive(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    assert!(cx.debug_bounds("nav-back").is_some());
    assert!(cx.debug_bounds("nav-forward-disabled").is_some());

    click(&mut cx, "nav-back");
    assert!(cx.debug_bounds("nav-back-disabled").is_some());
    assert!(cx.debug_bounds("nav-forward").is_some());
}

#[gpui::test]
fn the_arrows_start_each_strip_outside_its_scroll(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let (nav, strip) = (bounds(&mut cx, "nav"), bounds(&mut cx, "group-strip"));
    assert!(strip.left() <= nav.left() && strip.right() >= nav.right());
    assert!(strip.top() <= nav.top() && strip.bottom() >= nav.bottom());
    assert_eq!(nav.right(), bounds(&mut cx, "tab-bar").left());
    assert!(cx.debug_bounds("nav-row").is_none());
    assert_eq!(
        bounds(&mut cx, "note-column").top(),
        bounds(&mut cx, "tab-row").bottom()
    );

    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
    let (nav, header) = (bounds(&mut cx, "nav-1"), bounds(&mut cx, "group-header-1"));
    assert!(header.left() <= nav.left() && header.right() >= nav.right());
    assert!(header.top() <= nav.top() && header.bottom() >= nav.bottom());
    assert_eq!(nav.right(), bounds(&mut cx, "tab-bar-1").left());
}

#[gpui::test]
fn the_arrows_sit_level_with_their_rows_buttons(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let middle = |cx: &mut VisualTestContext, selector| bounds(cx, selector).center().y;
    assert_eq!(
        middle(&mut cx, "nav-forward-disabled"),
        middle(&mut cx, "dock-toggle-left")
    );

    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
    assert_eq!(
        middle(&mut cx, "nav-forward-1-disabled"),
        middle(&mut cx, "tab-new-1")
    );
}

#[gpui::test]
fn each_groups_arrows_light_and_walk_its_own_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    splits::split(&mut cx);
    assert!(cx.debug_bounds("nav-back").is_some());
    assert!(cx.debug_bounds("nav-back-1-disabled").is_some());

    click(&mut cx, "nav-back");

    assert_eq!(
        splits::last(&window, &mut cx),
        splits::centre(&window, &mut cx)[0]
    );
    assert_eq!(splits::shown_in(&window, &mut cx, 0), Some(a));
    assert_eq!(splits::shown_in(&window, &mut cx, 1), Some(b));
    assert!(cx.debug_bounds("nav-forward").is_some());
    assert!(cx.debug_bounds("nav-forward-1-disabled").is_some());
    splits::close(&mut cx);
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector).expect("drawn")
}
