//! Closing a group's last tab removes the group, except the centre's last (splits spec, L6).

use gpui::TestAppContext;

use super::splits::{
    centre, check_layout, close, current, editor_in, focused, last, shown_in, split, tabs_in,
};
use super::{ab_focused, keys, open, start_open};

#[gpui::test]
fn closing_a_splits_only_tab_removes_its_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let groups = centre(&window, &mut cx);

    keys(&mut cx, "cmd-w");

    assert_eq!(centre(&window, &mut cx), [groups[0]]);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn the_left_neighbour_takes_the_room(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let left = cx.debug_bounds("note-column").expect("drawn");
    let right = cx.debug_bounds("note-column-1").expect("drawn");

    keys(&mut cx, "cmd-w");

    let column = cx.debug_bounds("note-column").expect("drawn");
    assert!(cx.debug_bounds("note-column-1").is_none());
    assert!((column.left() - left.left()).abs() < gpui::px(0.01));
    assert!(
        (column.right() - right.right()).abs() < gpui::px(0.01),
        "{column:?} vs {left:?} and {right:?}"
    );
    close(&mut cx);
}

#[gpui::test]
fn closing_the_active_group_activates_and_focuses_its_heir(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    open(&window, &mut cx, b);
    assert_eq!(current(&window, &mut cx), Some(b));

    keys(&mut cx, "cmd-w");

    let groups = centre(&window, &mut cx);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(current(&window, &mut cx), Some(a));
    let heir = editor_in(&window, &mut cx, 0);
    assert!(focused(&window, &mut cx, &heir));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn cmd_shift_t_reopens_a_closed_split_tab_in_the_heir(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w");

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(b), Some(a)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(b));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn closing_a_group_drops_its_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let gate = fs.block_load("/vault/b.md");
    start_open(&window, &mut cx, b);

    keys(&mut cx, "cmd-w");
    gate.release();
    cx.run_until_parked();

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(current(&window, &mut cx), Some(a));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn closing_the_centres_last_tab_leaves_an_empty_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    keys(&mut cx, "cmd-w");

    keys(&mut cx, "cmd-w");

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(tabs_in(&window, &mut cx, 0), [None]);
    assert_eq!(current(&window, &mut cx), None);
    check_layout(&window, &mut cx);
    close(&mut cx);
}
