//! Which edge of a slot a tab or a view dragged over it lights (docks spec, Drag).

use std::cmp::Ordering;

use inkra_layout::{GroupId, Layout, Side, View};
use inkra_tabs::Tabs;

use crate::shell::dock_drag::DraggedView;
use crate::shell::tab_bar::DraggedTab;

/// The dragged item stood at index 1 of `own`.
fn lands_beside_its_own_slot_and_before_a_foreign_one(
    own: GroupId,
    other: GroupId,
    landing: impl Fn(GroupId, usize) -> Ordering,
) {
    assert_eq!(landing(own, 0), Ordering::Less);
    assert_eq!(landing(own, 1), Ordering::Equal);
    assert_eq!(landing(own, 2), Ordering::Greater);
    for ix in 0..3 {
        assert_eq!(landing(other, ix), Ordering::Less);
    }
}

#[test]
fn a_view_lands_beside_its_own_slot_and_before_a_foreign_one() {
    let layout = Layout::default();
    let (own, other) = (layout.dock_groups(Side::Left)[0], layout.last_center());
    let dragged = DraggedView {
        view: View::Files,
        index: 1,
        group: own,
    };

    lands_beside_its_own_slot_and_before_a_foreign_one(own, other, |group, ix| {
        dragged.landing(group, ix)
    });
}

#[test]
fn a_tab_lands_beside_its_own_slot_and_before_a_foreign_one() {
    let layout = Layout::default();
    let (own, other) = (layout.last_center(), layout.dock_groups(Side::Left)[0]);
    let dragged = DraggedTab {
        id: Tabs::<()>::new().active(),
        index: 1,
        title: "a".into(),
        group: own,
    };

    lands_beside_its_own_slot_and_before_a_foreign_one(own, other, |group, ix| {
        dragged.landing(group, ix)
    });
}
