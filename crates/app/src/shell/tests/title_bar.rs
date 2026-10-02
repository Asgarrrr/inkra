//! The tab row as the window's title bar: its layout and the window drag (shell chrome spec, S3).

use gpui::{
    Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle, point, px, size,
};

use super::{
    AppShell, ab_focused, click_row, editor_focused, four_notes, keys, no_vault, open, selected,
    tab_notes,
};
use crate::theme;

fn armed(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.title_drag_armed)
        .unwrap()
}

/// Thirty tabs in a 700 px window, the last one active: they overflow the bar.
fn thirty_tabs(cx: &mut TestAppContext) -> VisualTestContext {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    keys(&mut cx, "cmd-9");
    cx
}

#[gpui::test]
fn the_tab_row_spans_the_window_above_every_column(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let viewport = cx.update(|window, _| window.viewport_size());

    let row = cx.debug_bounds("tab-row").unwrap();
    assert_eq!(row.origin, point(px(0.), px(0.)));
    assert_eq!(row.size, size(viewport.width, theme::TITLE_BAR_HEIGHT));
    for column in ["sidebar-column", "note-column", "backlinks-column"] {
        let top = cx.debug_bounds(column).unwrap().top();
        assert_eq!(top, row.bottom(), "{column}");
    }
}

#[gpui::test]
fn the_traffic_lights_have_their_space(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    let lights = cx.debug_bounds("title-drag-start").unwrap();
    let toggle = cx.debug_bounds("panel-toggle-sidebar").unwrap();
    assert_eq!(lights.size.width, theme::TRAFFIC_LIGHT_PADDING);
    assert_eq!(toggle.left(), lights.right());

    window
        .update(&mut cx, |_, window, _| {
            window.toggle_fullscreen();
            window.refresh();
        })
        .unwrap();
    cx.run_until_parked();

    let lights = cx.debug_bounds("title-drag-start").unwrap();
    assert_eq!(lights.size.width, theme::ROW_INSET);
}

/// The bar is as wide as its tabs, so the row's empty space is the region's, not the bar's.
#[gpui::test]
fn the_space_after_the_tabs_is_a_drag_region(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");

    let bar = cx.debug_bounds("tab-bar").unwrap();
    let region = cx.debug_bounds("title-drag-end").unwrap();
    let toggle = cx.debug_bounds("panel-toggle-backlinks").unwrap();
    assert_eq!(bar.right(), cx.debug_bounds("tab-1").unwrap().right());
    assert_eq!(region.left(), bar.right());
    assert_eq!(region.right(), toggle.left());
}

#[gpui::test]
fn overflowing_tabs_keep_a_drag_region(cx: &mut TestAppContext) {
    let mut cx = thirty_tabs(cx);

    let region = cx.debug_bounds("title-drag-end").unwrap();
    assert!(
        region.size.width >= theme::TITLE_DRAG_MIN_WIDTH,
        "{region:?}"
    );
    let toggle = cx.debug_bounds("panel-toggle-backlinks").unwrap();
    assert!(
        toggle.right() <= px(700.),
        "the toggle is pushed out: {toggle:?}"
    );
    let bar = cx.debug_bounds("tab-bar").unwrap();
    let last = cx.debug_bounds("tab-29").unwrap();
    assert!(
        last.left() >= bar.left() && last.right() <= bar.right(),
        "tab-29 at {last:?} is outside the bar at {bar:?}"
    );
}

#[gpui::test]
fn a_press_on_a_drag_region_keeps_the_editors_focus(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let at = cx.debug_bounds("title-drag-end").unwrap().center();

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(armed(&window, &mut cx));
    assert!(editor_focused(&window, &mut cx));

    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(!armed(&window, &mut cx));
}

/// A release outside the window reaches no listener; the next move, with no button held, must not move it.
#[gpui::test]
fn a_release_outside_the_window_leaves_no_move_armed(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);
    let at = cx.debug_bounds("title-drag-end").unwrap().center();

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(
        point(px(-10.), px(-10.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());

    assert!(!armed(&window, &mut cx));
}

/// The test platform leaves `start_window_move` `unimplemented!()`: the panic is the move.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn a_drag_on_a_drag_region_moves_the_window(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let at = cx.debug_bounds("title-drag-end").unwrap().center();

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        at + point(px(1.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
}

/// The first move is already off the region: only the root sees it.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn a_fast_drag_off_a_drag_region_moves_the_window(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let at = cx.debug_bounds("title-drag-start").unwrap().center();

    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(px(500.), px(300.)),
        MouseButton::Left,
        Modifiers::none(),
    );
}

#[gpui::test]
fn a_tab_drag_never_arms_a_window_move(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    let from = cx.debug_bounds("tab-0").unwrap().center();
    let to = cx.debug_bounds("tab-2").unwrap().center();

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    assert!(!armed(&window, &mut cx));
    cx.simulate_mouse_move(
        from + point(px(4.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx).0, [Some(b), Some(c), Some(a)]);
}

#[gpui::test]
fn no_vault_draws_no_sidebar_column(cx: &mut TestAppContext) {
    let (_window, mut cx) = no_vault(cx);

    let row = cx.debug_bounds("tab-row").expect("the row is drawn");
    assert!(cx.debug_bounds("sidebar-column").is_none());
    let note = cx.debug_bounds("note-column").unwrap();
    assert_eq!(note.origin, point(px(0.), row.bottom()));
}

/// A tree click lands where `row_center` reads; a header of the wrong height moves every row.
#[gpui::test]
fn the_panel_headers_line_up_under_the_title_row(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let row = cx.debug_bounds("tab-row").unwrap();
    for header in ["panel-header-sidebar", "panel-header-backlinks"] {
        let bounds = cx.debug_bounds(header).unwrap();
        assert_eq!(bounds.top(), row.bottom(), "{header}");
        assert_eq!(
            bounds.bottom(),
            row.bottom() + theme::PANEL_HEADER_HEIGHT,
            "{header}"
        );
    }
}

#[gpui::test]
fn a_click_at_row_center_lands_on_its_row(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [_, b, ..]) = four_notes(cx);

    click_row(&mut cx, 1., Modifiers::none());

    assert_eq!(selected(&window, &mut cx), Some(b));
}
