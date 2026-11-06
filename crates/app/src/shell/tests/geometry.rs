//! A group's pixel rect: each surface gives its own origin and room (splits spec, P3).

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{GroupId, Side, View};

use super::splits::{close, last};
use super::{ab_focused, open};
use crate::shell::AppShell;

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
            shell.reveal(View::Backlinks, window, cx);
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
