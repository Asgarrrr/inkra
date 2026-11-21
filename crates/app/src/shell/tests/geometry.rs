//! A group's pixel rect: each surface gives its own origin and room (splits spec, P3).

use std::sync::Arc;

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Axis, GroupId, Side};
use inkra_vault::mem_fs::MemFs;

use super::splits::{close, last};
use super::{ab_focused, no_vault, open};
use crate::backlinks::BACKLINKS;
use crate::shell::AppShell;
use crate::theme;

fn bounds_of(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    group: GroupId,
) -> Bounds<Pixels> {
    window
        .update(cx, |shell, window, _| shell.bounds_of(group, window))
        .unwrap()
        .expect("the group is in the layout")
}

fn near(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    let close = |a: Pixels, b: Pixels| (a - b).abs() <= gpui::px(1.);
    close(a.left(), b.left())
        && close(a.top(), b.top())
        && close(a.size.width, b.size.width)
        && close(a.size.height, b.size.height)
}

#[gpui::test]
fn a_group_fills_the_room_of_its_own_surface(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    window
        .update(&mut cx, |shell, window, cx| {
            shell.reveal(BACKLINKS, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let (left, right) = window
        .update(&mut cx, |shell, _, _| {
            (
                shell.layout.dock_groups(Side::Left)[0],
                shell.layout.dock_groups(Side::Right)[0],
            )
        })
        .unwrap();
    let centre = last(&window, &mut cx);

    for (group, drawn) in [
        (left, "dock-left"),
        (right, "dock-right"),
        (centre, "note-column"),
    ] {
        let expected = cx.debug_bounds(drawn).expect("drawn");
        let bounds = bounds_of(&window, &mut cx, group);
        assert!(near(bounds, expected), "{drawn}: {bounds:?} {expected:?}");
    }
    close(&mut cx);
}

#[gpui::test]
fn the_right_dock_counts_both_insets(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    window
        .update(&mut cx, |shell, window, cx| {
            shell.reveal(BACKLINKS, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let (views, min) = window
        .update(&mut cx, |shell, window, _| {
            let group = shell.layout.dock_groups(Side::Right)[0];
            (
                shell.group(group).views().count(),
                shell.min_sizes(window)[&(group, Axis::Horizontal)],
            )
        })
        .unwrap();

    assert_eq!(views, 1);
    let expected = f32::from(
        theme::ICON_BUTTON_WIDTH
            + theme::TITLE_DRAG_MIN_WIDTH
            + theme::ROW_INSET * 2.
            + theme::ICON_BUTTON_WIDTH,
    );
    assert!((min - expected).abs() < 0.5, "{min} {expected}");
    close(&mut cx);
}

#[gpui::test]
fn no_vault_counts_the_lights_and_no_toggle(cx: &mut TestAppContext) {
    let (window, mut cx) = no_vault(cx, &Arc::new(MemFs::new()));
    let min = window
        .update(&mut cx, |shell, window, _| {
            let centre = shell.layout.center_groups()[0];
            shell.min_sizes(window)[&(centre, Axis::Horizontal)]
        })
        .unwrap();

    let expected = f32::from(
        theme::TAB_CURVE * 2.
            + (theme::ROW_INSET + theme::ICON_BUTTON_WIDTH * 2.)
            + theme::TAB_MIN_WIDTH
            + theme::ICON_BUTTON_WIDTH
            + theme::TITLE_DRAG_MIN_WIDTH
            + theme::TRAFFIC_LIGHT_PADDING
            + theme::ROW_INSET,
    );
    assert!((min - expected).abs() < 0.5, "{min} {expected}");
}

#[gpui::test]
fn no_vault_gives_the_centre_the_whole_row(cx: &mut TestAppContext) {
    let (window, mut cx) = no_vault(cx, &Arc::new(MemFs::new()));
    let centre = window
        .update(&mut cx, |shell, _, _| shell.layout.center_groups()[0])
        .unwrap();

    let bounds = bounds_of(&window, &mut cx, centre);
    let expected = cx.debug_bounds("note-column").expect("drawn");
    assert!(near(bounds, expected), "{bounds:?} {expected:?}");
}
