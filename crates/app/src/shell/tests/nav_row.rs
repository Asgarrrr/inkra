use gpui::{Modifiers, MouseButton, TestAppContext, point, px};

use super::tab_bar::click;
use super::{ab_focused, editor_focused, open, showing, tab_notes};
use crate::theme;

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
fn a_press_beside_the_arrows_keeps_the_editors_focus(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let row = cx.debug_bounds("nav-row").unwrap();
    let at = point(row.right() - px(40.), row.center().y);

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn the_nav_row_tops_the_note_column_level_with_the_backlinks_header(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let row = cx.debug_bounds("nav-row").unwrap();
    let column = cx.debug_bounds("note-column").unwrap();
    let header = cx.debug_bounds("panel-header-backlinks").unwrap();

    assert_eq!(row.top(), column.top());
    assert_eq!(row.left(), column.left());
    assert_eq!(row.size.height, theme::NAV_ROW_HEIGHT);
    assert_eq!(row.bottom(), header.bottom());
}
