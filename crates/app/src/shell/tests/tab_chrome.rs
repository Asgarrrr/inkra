//! The tab's look: its ×, its dirty dot, its hover and its drag preview (shell chrome spec, S1).

use gpui::{Modifiers, MouseButton, TestAppContext, point, px};

use super::tab_bar::click;
use super::{ab_focused, keys, open, tab_notes};

/// No mouse move first: an `invisible()` × registers no listener until a hover frame draws it (`div.rs:2529`).
#[gpui::test]
fn an_inactive_tabs_close_button_closes_it_without_a_hover(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-1");

    click(&mut cx, "tab-close-1");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
}

/// `a` and `b` are one letter: without a minimum title width, each tab's centre lies on its ×.
#[gpui::test]
fn a_click_on_a_short_tabs_centre_never_closes_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    click(&mut cx, "tab-0");
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b)], 0),
        "the inactive tab"
    );

    click(&mut cx, "tab-0");
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b)], 0),
        "the active tab"
    );
}

#[gpui::test]
fn a_dirty_note_shows_a_dot_on_its_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    assert!(cx.debug_bounds("tab-dot-0").is_none(), "a clean note");

    cx.simulate_input("x");
    cx.run_until_parked();
    assert!(cx.debug_bounds("tab-dot-0").is_some(), "the edit draws it");

    keys(&mut cx, "cmd-s");
    assert!(cx.debug_bounds("tab-dot-0").is_none(), "the save clears it");
}

/// The × shows on hover by opacity, so it takes its room whether shown or not.
#[gpui::test]
fn the_tab_keeps_its_width_when_hovered(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-1");
    let before = cx.debug_bounds("tab-1").expect("drawn");

    cx.simulate_mouse_move(before.center(), None, Modifiers::none());
    cx.run_until_parked();

    let after = cx.debug_bounds("tab-1").expect("drawn");
    assert_eq!(after.size.width, before.size.width);
}

/// The preview has no ×, so a × box taller than the line box would make the two differ.
#[gpui::test]
fn the_preview_matches_the_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let from = cx.debug_bounds("tab-0").expect("drawn").center();

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(4.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();

    let tab = cx.debug_bounds("tab-0").expect("drawn");
    let preview = cx
        .debug_bounds("tab-preview")
        .expect("the preview is drawn");
    assert_eq!(preview.size.height, tab.size.height);
}
