//! The tab's look: its ×, its dirty and failed dots, its hover and its drag preview (shell chrome spec, S1).

use gpui::{Modifiers, MouseButton, TestAppContext, point, px};

use super::session_writes::wait;
use super::tab_bar::click;
use super::{ab_focused, keys, open, tab_notes};
use crate::theme;
use crate::vault::AUTOSAVE_DELAY;

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

#[gpui::test]
fn a_failed_save_shows_the_failed_dot_instead_of_the_dirty_dot(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = ab_focused(cx);
    fs.make_unwritable("/vault/a.md");
    open(&window, &mut cx, a);

    cx.simulate_input("x");
    wait(&mut cx, AUTOSAVE_DELAY);

    assert!(cx.debug_bounds("tab-failed-0").is_some());
    assert!(cx.debug_bounds("tab-dot-0").is_none());
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

/// Every tab stands on the row's floor, so the active one runs on into the note (D3).
#[gpui::test]
fn the_active_tab_reaches_the_row_bottom(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");

    let row = cx.debug_bounds("tab-row").unwrap();
    let bar = cx.debug_bounds("tab-bar").unwrap();
    let first = cx.debug_bounds("tab-0").unwrap();
    let second = cx.debug_bounds("tab-1").unwrap();
    assert_eq!(second.size.height, theme::TAB_HEIGHT);
    assert_eq!(second.bottom(), row.bottom());
    assert_eq!(first.bottom(), row.bottom());
    assert_eq!(first.left(), bar.left() + theme::TAB_CURVE);
    assert_eq!(second.left(), first.right() + theme::TAB_GAP);
}

#[gpui::test]
fn the_new_tab_button_sits_on_the_tabs_centreline(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    let new = cx.debug_bounds("tab-new").unwrap();
    let close = cx.debug_bounds("tab-close-0").unwrap();
    assert_eq!(new.center().y, close.center().y);
    assert_eq!(new.left(), cx.debug_bounds("tab-bar").unwrap().right());
}

#[gpui::test]
fn the_new_tab_button_opens_a_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    click(&mut cx, "tab-new");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), None], 1));
}

#[test]
fn the_tab_close_radius_is_concentric() {
    assert_eq!(
        theme::TAB_CLOSE_RADIUS,
        theme::TAB_HOVER_RADIUS
            - (theme::TAB_HEIGHT
                - theme::RULE_WIDTH
                - theme::TAB_HOVER_INSET
                - theme::TAB_CLOSE_SIZE)
                / 2.
    );
}

/// An inactive tab's hover is a pill inset in the tab, clear of the row's bottom, as the reference's.
#[gpui::test]
fn an_inactive_tabs_hover_is_an_inset_pill(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");

    let tab = cx.debug_bounds("tab-0").unwrap();
    let pill = cx.debug_bounds("tab-hover").unwrap();
    assert_eq!(pill.left(), tab.left() + theme::TAB_HOVER_INSET);
    assert_eq!(pill.right(), tab.right() - theme::TAB_HOVER_INSET);
    assert_eq!(pill.top(), tab.top() + theme::RULE_WIDTH);
    assert_eq!(pill.bottom(), tab.bottom() - theme::TAB_HOVER_INSET);
}

/// The title and the × centre on the hover pill, not on the taller tab, as the reference's.
#[gpui::test]
fn an_inactive_tabs_content_centres_on_its_pill(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");

    let pill = cx.debug_bounds("tab-hover").unwrap();
    let close = cx.debug_bounds("tab-close-0").unwrap();
    assert_eq!(close.size.height, theme::TAB_CLOSE_SIZE);
    assert_eq!(close.center().y, pill.center().y);
}
