//! Split Down: the centre as a tree, a lower group's header, the title row's top strips (splits spec, D4).

use gpui::{Bounds, MouseButton, Pixels, TestAppContext, VisualTestContext, WindowHandle, px};
use inkra_layout::{Axis, Member, Surface};

use super::dock_menu::drawn;
use super::splits::{centre, check_layout, close, last, shown_in, split};
use super::tab_bar::{click, press};
use super::{AppShell, ab_focused, keys, open};
use crate::shell::group::SplitDown;
use crate::theme;

fn split_down(cx: &mut VisualTestContext) {
    cx.dispatch_action(SplitDown);
    cx.run_until_parked();
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector).expect("drawn")
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() < px(0.01)
}

fn center(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Member {
    window
        .update(cx, |shell, _, _| shell.layout.center().clone())
        .unwrap()
}

#[gpui::test]
fn split_down_puts_the_new_group_below(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    split_down(&mut cx);

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    let (upper, lower) = (
        bounds(&mut cx, "note-column"),
        bounds(&mut cx, "note-column-1"),
    );
    assert!(near(upper.left(), lower.left()), "{upper:?} vs {lower:?}");
    assert!(near(upper.right(), lower.right()), "{upper:?} vs {lower:?}");
    assert!(near(upper.bottom(), lower.top()), "{upper:?} vs {lower:?}");
    assert!(
        near(upper.size.height, lower.size.height),
        "{upper:?} vs {lower:?}"
    );
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_lower_group_draws_its_strip_in_its_header(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split_down(&mut cx);

    let header = bounds(&mut cx, "group-header-1");
    let strip = bounds(&mut cx, "tab-bar-1");
    let column = bounds(&mut cx, "note-column-1");

    assert!(!drawn(&mut cx, "group-header"));
    assert!(near(header.size.height, theme::GROUP_HEADER_HEIGHT));
    assert!(near(header.top(), column.top()), "{header:?} vs {column:?}");
    assert!(near(header.left(), column.left()));
    assert!(near(header.right(), column.right()));
    assert!(
        header.contains(&strip.origin) && strip.bottom() <= header.bottom(),
        "{strip:?} outside {header:?}"
    );
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn the_title_row_holds_only_the_top_groups_strips(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    split_down(&mut cx);
    assert_eq!(centre(&window, &mut cx).len(), 3);

    let row = bounds(&mut cx, "title-tabs");
    for top in ["tab-bar", "tab-bar-1"] {
        let strip = bounds(&mut cx, top);
        assert!(strip.bottom() <= row.bottom(), "{top} at {strip:?}");
    }
    let lower = bounds(&mut cx, "tab-bar-2");
    assert!(lower.top() >= row.bottom(), "{lower:?} in the title row");
    assert!(drawn(&mut cx, "group-header-2"));
    assert!(!drawn(&mut cx, "group-header-1"));
    close(&mut cx);
}

#[gpui::test]
fn each_top_strip_sits_over_its_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    split(&mut cx);
    split_down(&mut cx);
    assert!(drawn(&mut cx, "group-header-3"));
    let widths: Vec<f32> = window
        .update(&mut cx, |shell, _, _| shell.layout.rects(Surface::Center))
        .unwrap()
        .into_iter()
        .map(|(_, rect)| rect.width)
        .collect();
    assert_eq!(widths, [0.5, 0.25, 0.25, 0.25]);

    for (strip, column) in [
        ("group-strip", "note-column"),
        ("group-strip-1", "note-column-1"),
        ("group-strip-2", "note-column-2"),
    ] {
        let (strip, column) = (bounds(&mut cx, strip), bounds(&mut cx, column));
        assert!(near(strip.left(), column.left()), "{strip:?} vs {column:?}");
        assert!(
            near(strip.right(), column.right()),
            "{strip:?} vs {column:?}"
        );
    }
    close(&mut cx);
}

#[gpui::test]
fn closing_a_nested_groups_last_tab_collapses_its_split(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    split_down(&mut cx);
    let groups = centre(&window, &mut cx);

    keys(&mut cx, "cmd-w");

    assert_eq!(
        center(&window, &mut cx),
        Member::Split {
            axis: Axis::Horizontal,
            children: vec![Member::Group(groups[0]), Member::Group(groups[1])],
            flexes: vec![1.0, 1.0],
        }
    );
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert!(!drawn(&mut cx, "group-header-1"));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn the_tab_menu_lists_split_down(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    press(&mut cx, "tab-0", MouseButton::Right);
    assert!(drawn(&mut cx, "context-menu-item-2"));
    assert!(!drawn(&mut cx, "context-menu-item-3"));
    click(&mut cx, "context-menu-item-1");

    let groups = centre(&window, &mut cx);
    assert_eq!(
        center(&window, &mut cx),
        Member::Split {
            axis: Axis::Vertical,
            children: vec![Member::Group(groups[0]), Member::Group(groups[1])],
            flexes: vec![1.0, 1.0],
        }
    );
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    check_layout(&window, &mut cx);
    close(&mut cx);
}
