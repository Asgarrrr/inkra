//! A separator's drag, its minimum, its double-click, and the split refused for room (splits spec, P2).

use gpui::{
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, WindowHandle, point, px, size,
};
use inkra_layout::{Member, Side, SplitPath, Surface};

use super::dragging::drag_to;
use super::edge_drops::hold;
use super::splits::{centre, close, last, split};
use super::tab_bar::{click, press};
use super::{ab_focused, keys, open};
use crate::shell::AppShell;
use crate::shell::docks::MoveBacklinksToOtherDock;
use crate::shell::group::SplitDown;
use crate::theme;

fn flexes(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    surface: Surface,
) -> Vec<f32> {
    window
        .update(cx, |shell, _, _| {
            match shell.layout.member_at(&SplitPath {
                surface,
                steps: Vec::new(),
            }) {
                Some(Member::Split { flexes, .. }) => flexes.clone(),
                other => panic!("no split at {surface:?}'s root: {other:?}"),
            }
        })
        .unwrap()
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Bounds<Pixels> {
    cx.debug_bounds(selector).expect("drawn")
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

/// The group-strip chrome a centre group keeps with both docks drawn.
fn centre_minimum() -> Pixels {
    theme::TAB_CURVE * 2.
        + theme::TAB_MIN_WIDTH
        + theme::ICON_BUTTON_WIDTH
        + theme::TITLE_DRAG_MIN_WIDTH
}

fn x_to(cx: &mut VisualTestContext, selector: &'static str, x: Pixels) -> Point<Pixels> {
    point(x, bounds(cx, selector).center().y)
}

fn y_to(cx: &mut VisualTestContext, selector: &'static str, y: Pixels) -> Point<Pixels> {
    point(bounds(cx, selector).center().x, y)
}

#[gpui::test]
fn dragging_the_separator_resizes_both_groups(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let boundary = bounds(&mut cx, "note-column-1").left();

    let to = x_to(&mut cx, "separator-1", boundary + px(100.));
    drag_to(&mut cx, "separator-1", to);

    let (left, right) = (
        bounds(&mut cx, "note-column").size.width,
        bounds(&mut cx, "note-column-1").size.width,
    );
    assert!(near(left - right, px(200.)), "{left:?} {right:?}");
    let flexes = flexes(&window, &mut cx, Surface::Center);
    assert!(
        (flexes.iter().sum::<f32>() - 2.0).abs() < 1e-3,
        "{flexes:?}"
    );
    close(&mut cx);
}

#[gpui::test]
fn a_separator_stops_at_the_minimum(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let boundary = bounds(&mut cx, "note-column-1").left();

    let to = x_to(&mut cx, "separator-1", boundary - px(2000.));
    drag_to(&mut cx, "separator-1", to);

    let width = bounds(&mut cx, "note-column").size.width;
    assert!(near(width, centre_minimum()), "{width:?}");
    let (strip, new_tab) = (bounds(&mut cx, "group-strip"), bounds(&mut cx, "tab-new"));
    assert!(
        strip.contains(&new_tab.origin) && new_tab.right() <= strip.right(),
        "{new_tab:?} in {strip:?}"
    );
    close(&mut cx);
}

#[gpui::test]
fn double_clicking_a_separator_equalises(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let boundary = bounds(&mut cx, "note-column-1").left();
    let to = x_to(&mut cx, "separator-1", boundary + px(100.));
    drag_to(&mut cx, "separator-1", to);
    assert_ne!(flexes(&window, &mut cx, Surface::Center), [1.0, 1.0]);

    let at = bounds(&mut cx, "separator-1").center();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    cx.run_until_parked();

    assert_eq!(flexes(&window, &mut cx, Surface::Center), [1.0, 1.0]);
    close(&mut cx);
}

#[gpui::test]
fn a_press_on_a_separator_activates_no_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let active = last(&window, &mut cx);
    assert_eq!(active, centre(&window, &mut cx)[1]);

    let at = bounds(&mut cx, "separator-1").center() - point(px(1.), px(0.));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(last(&window, &mut cx), active);
    close(&mut cx);
}

#[gpui::test]
fn the_vertical_separator_resizes_rows(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
    let boundary = bounds(&mut cx, "note-column-1").top();

    let to = y_to(&mut cx, "separator-1", boundary + px(100.));
    drag_to(&mut cx, "separator-1", to);

    let (top, bottom) = (
        bounds(&mut cx, "note-column").size.height,
        bounds(&mut cx, "note-column-1").size.height,
    );
    assert!(near(top - bottom, px(200.)), "{top:?} {bottom:?}");
    close(&mut cx);
}

#[gpui::test]
fn split_right_is_refused_when_a_half_would_be_too_narrow(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_resize(size(px(600.), px(600.)));
    cx.run_until_parked();

    split(&mut cx);

    assert_eq!(centre(&window, &mut cx).len(), 1);
    close(&mut cx);
}

#[gpui::test]
fn a_dock_separator_resizes_the_dock_groups(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.dispatch_action(MoveBacklinksToOtherDock);
    cx.run_until_parked();
    press(&mut cx, "dock-icon-backlinks", MouseButton::Right);
    click(&mut cx, "context-menu-item-2");
    let boundary = bounds(&mut cx, "dock-left-group-1").top();

    let to = y_to(&mut cx, "dock-left-separator-1", boundary + px(100.));
    drag_to(&mut cx, "dock-left-separator-1", to);

    let (top, bottom) = (
        bounds(&mut cx, "dock-left-group").size.height,
        bounds(&mut cx, "dock-left-group-1").size.height,
    );
    assert!(near(top - bottom, px(200.)), "{top:?} {bottom:?}");
    let flexes = flexes(&window, &mut cx, Surface::Dock(Side::Left));
    assert!(
        (flexes.iter().sum::<f32>() - 2.0).abs() < 1e-3,
        "{flexes:?}"
    );
    close(&mut cx);
}

/// Closing two of four groups mid-drag leaves the held separator past the split's end.
#[gpui::test]
fn a_drag_whose_separator_is_gone_ends(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_resize(size(px(2400.), px(1200.)));
    cx.run_until_parked();
    for _ in 0..3 {
        split(&mut cx);
    }
    assert_eq!(centre(&window, &mut cx).len(), 4);
    let from = bounds(&mut cx, "separator-3").center();
    hold(&mut cx, "separator-3", from);
    keys(&mut cx, "cmd-w");
    keys(&mut cx, "cmd-w");

    cx.simulate_mouse_move(
        from - point(px(40.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );

    assert!(!cx.update(|_, cx| cx.has_active_drag()), "the drag ended");
    assert_eq!(flexes(&window, &mut cx, Surface::Center).len(), 2);
    cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    close(&mut cx);
}
