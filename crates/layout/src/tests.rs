use std::collections::HashSet;

use proptest::prelude::*;

use super::{GroupId, Layout, Side, StoredDock, View, restore_views};

#[test]
fn the_default_puts_files_left_and_backlinks_right_both_shown() {
    let layout = Layout::default();
    assert_eq!(View::Files.default_side(), Side::Left);
    assert_eq!(View::Backlinks.default_side(), Side::Right);
    assert_eq!(Side::Left.other(), Side::Right);
    assert_eq!(Side::Right.other(), Side::Left);
    for side in [Side::Left, Side::Right] {
        let group = layout.dock(side).expect("a group in each dock");
        assert_eq!(layout.side_of(group), Some(side));
        assert!(layout.is_shown(side));
        assert!(layout.is_drawn(side));
    }
    assert_eq!(layout.center(), [GroupId(0)]);
    assert_eq!(layout.flexes(), [1.0]);
    assert_eq!(layout.last_center(), GroupId(0));
    assert_eq!(layout.side_of(GroupId(0)), None);
    assert_eq!(
        layout.groups().map(GroupId::raw).collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn toggle_flips_only_its_side() {
    let mut layout = Layout::default();
    layout.toggle(Side::Left);
    assert!(!layout.is_shown(Side::Left));
    assert!(layout.is_shown(Side::Right));
    layout.toggle(Side::Right);
    assert!(!layout.is_shown(Side::Left));
    assert!(!layout.is_shown(Side::Right));
}

#[test]
fn toggling_twice_restores_the_layout() {
    let mut layout = Layout::default();
    layout.toggle(Side::Right);
    layout.toggle(Side::Right);
    assert_eq!(layout, Layout::default());
}

#[test]
fn a_hidden_dock_shows_no_visible_view() {
    let mut layout = Layout::default();
    layout.toggle(Side::Left);
    assert!(!layout.is_drawn(Side::Left));
    assert!(layout.is_drawn(Side::Right));
    layout.show(Side::Left);
    assert!(layout.is_drawn(Side::Left));
    layout.show(Side::Left);
    assert!(layout.is_shown(Side::Left));
}

#[test]
fn an_empty_dock_is_not_drawn_and_keeps_its_flag() {
    let mut layout = Layout::default();
    let left = layout.dock(Side::Left).expect("a left group");
    layout.remove(left);
    assert_eq!(layout.dock(Side::Left), None);
    assert!(layout.is_shown(Side::Left));
    assert!(!layout.is_drawn(Side::Left));

    layout.toggle(Side::Left);
    assert!(!layout.is_shown(Side::Left));
    layout.toggle(Side::Right);
    assert!(!layout.is_drawn(Side::Right));
}

#[test]
fn add_dock_gives_a_fresh_id_and_draws_the_dock() {
    let mut layout = Layout::default();
    let right = layout.dock(Side::Right).expect("a right group");
    layout.remove(right);
    let added = layout.add_dock(Side::Right);
    assert!(![GroupId(0), GroupId(1), right].contains(&added));
    assert_eq!(layout.dock(Side::Right), Some(added));
    assert_eq!(layout.side_of(added), Some(Side::Right));
    assert!(layout.is_drawn(Side::Right));
}

#[test]
fn removing_a_dock_group_keeps_its_flag() {
    let mut layout = Layout::default();
    layout.toggle(Side::Right);
    let right = layout.dock(Side::Right).expect("a right group");
    layout.remove(right);
    assert!(!layout.is_shown(Side::Right));
    assert_eq!(
        layout.groups().collect::<Vec<_>>(),
        [GroupId(0), GroupId(1)]
    );
    layout.add_dock(Side::Right);
    assert!(!layout.is_drawn(Side::Right));
}

#[test]
#[should_panic(expected = "the centre always has a group")]
fn removing_the_centre_panics() {
    let mut layout = Layout::default();
    layout.remove(layout.center()[0]);
}

#[test]
fn split_right_puts_the_new_group_after_and_halves_its_room() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout.split_right(first);
    assert_eq!(layout.center(), [first, second]);
    assert_eq!(layout.flexes(), [1.0, 1.0]);
    assert_eq!(layout.last_center(), first);
    assert_eq!(layout.side_of(second), None);

    let third = layout.split_right(first);
    assert_eq!(layout.center(), [first, third, second]);
    assert_eq!(layout.flexes(), [0.75, 0.75, 1.5]);
    let handed: HashSet<_> = layout.groups().collect();
    assert_eq!(handed.len(), 5);

    layout.activate(third);
    assert_eq!(layout.last_center(), third);
    assert_eq!(
        layout.groups().collect::<Vec<_>>(),
        [first, third, second, GroupId(1), GroupId(2)]
    );
}

#[test]
fn split_right_keeps_the_other_groups_widths() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout.split_right(first);
    layout.split_right(second);
    assert_eq!(layout.flexes(), [1.5, 0.75, 0.75]);
}

#[test]
#[should_panic(expected = "is not in the centre")]
fn activating_a_dock_group_panics() {
    let mut layout = Layout::default();
    layout.activate(GroupId(1));
}

#[test]
#[should_panic(expected = "is not in the centre")]
fn splitting_a_dock_group_panics() {
    let mut layout = Layout::default();
    layout.split_right(GroupId(2));
}

#[test]
#[should_panic(expected = "already holds a group")]
fn a_second_group_in_one_dock_panics() {
    Layout::default().add_dock(Side::Left);
}

fn state(views: &[View], active: usize) -> StoredDock {
    StoredDock {
        views: views.iter().map(|view| view.name().to_owned()).collect(),
        active,
        shown: true,
    }
}

/// Both views in the left dock, `active` the active one's index; the right dock empty.
fn both_left(active: usize) -> [(Vec<View>, usize); 2] {
    [
        (vec![View::Files, View::Backlinks], active),
        (Vec::new(), 0),
    ]
}

fn defaults() -> [(Vec<View>, usize); 2] {
    [(vec![View::Files], 0), (vec![View::Backlinks], 0)]
}

#[test]
fn a_view_in_both_docks_is_kept_left() {
    let views = restore_views(
        &state(&[View::Files, View::Backlinks], 1),
        &state(&[View::Backlinks], 0),
    );
    assert_eq!(views, both_left(1));
}

#[test]
fn a_view_twice_in_one_dock_is_kept_first() {
    let views = restore_views(
        &state(&[View::Files, View::Files], 0),
        &state(&[View::Backlinks], 0),
    );
    assert_eq!(views, defaults());
}

#[test]
fn a_missing_view_is_appended_to_its_default_side() {
    let views = restore_views(&state(&[], 0), &state(&[View::Backlinks], 0));
    assert_eq!(views, defaults());

    let views = restore_views(&state(&[View::Files], 0), &state(&[], 0));
    assert_eq!(views, defaults());

    let views = restore_views(&state(&[], 0), &state(&[View::Files], 0));
    assert_eq!(
        views,
        [(Vec::new(), 0), (vec![View::Files, View::Backlinks], 0)]
    );
}

#[test]
fn an_active_index_out_of_range_is_clamped() {
    let views = restore_views(&state(&[View::Files, View::Backlinks], 7), &state(&[], 3));
    assert_eq!(views, both_left(1));
}

#[test]
fn both_docks_empty_restore_the_default_views_with_their_flags() {
    let views = restore_views(&state(&[], 0), &state(&[], 0));
    assert_eq!(views, defaults());
}

fn stored(names: &[&str], active: usize) -> StoredDock {
    StoredDock {
        views: names.iter().map(|&name| name.to_owned()).collect(),
        active,
        shown: true,
    }
}

#[test]
fn an_unknown_name_before_the_active_keeps_the_active_view() {
    let [(views, active), _] = restore_views(
        &stored(&["graph", "files", "backlinks"], 1),
        &stored(&[], 0),
    );
    assert_eq!(views[active], View::Files);
}

#[test]
fn an_active_duplicate_maps_to_its_kept_view() {
    let [(views, active), _] = restore_views(
        &stored(&["files", "files", "backlinks"], 1),
        &stored(&[], 0),
    );
    assert_eq!(views[active], View::Files);
}

#[test]
fn views_are_stored_by_these_names() {
    assert_eq!(View::Files.name(), "files");
    assert_eq!(View::Backlinks.name(), "backlinks");
    for view in View::ALL {
        assert_eq!(View::named(view.name()), Some(view));
    }
    assert_eq!(View::named("graph"), None);
}

fn name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("files".to_owned()),
        Just("backlinks".to_owned()),
        "[a-z]{0,9}",
    ]
}

fn stored_dock() -> impl Strategy<Value = StoredDock> {
    (
        proptest::collection::vec(name(), 0..6),
        0..8usize,
        any::<bool>(),
    )
        .prop_map(|(views, active, shown)| StoredDock {
            views,
            active,
            shown,
        })
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Toggle(Side),
    Show(Side),
    Remove(Side),
    Add(Side),
}

#[derive(Debug, Clone, Copy)]
enum CentreOp {
    Dock(Op),
    Split(usize),
    Activate(usize),
}

fn centre_op() -> impl Strategy<Value = CentreOp> {
    prop_oneof![
        op().prop_map(CentreOp::Dock),
        any::<usize>().prop_map(CentreOp::Split),
        any::<usize>().prop_map(CentreOp::Activate),
    ]
}

fn fractions(layout: &Layout) -> Vec<(GroupId, f32)> {
    let sum: f32 = layout.flexes().iter().sum();
    layout
        .center()
        .iter()
        .zip(layout.flexes())
        .map(|(&group, flex)| (group, flex / sum))
        .collect()
}

fn apply(layout: &mut Layout, op: Op, handed: &mut HashSet<GroupId>) -> Result<(), TestCaseError> {
    match op {
        Op::Toggle(side) => layout.toggle(side),
        Op::Show(side) => {
            layout.show(side);
            prop_assert!(layout.is_shown(side));
        }
        Op::Remove(side) => {
            if let Some(group) = layout.dock(side) {
                layout.remove(group);
            }
        }
        Op::Add(side) => {
            if layout.dock(side).is_none() {
                let added = layout.add_dock(side);
                prop_assert!(handed.insert(added), "{added:?} handed out twice");
            }
        }
    }
    Ok(())
}

fn side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Left), Just(Side::Right)]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        side().prop_map(Op::Toggle),
        side().prop_map(Op::Show),
        side().prop_map(Op::Remove),
        side().prop_map(Op::Add),
    ]
}

proptest! {
    #[test]
    fn restore_repairs_any_stored_names(left in stored_dock(), right in stored_dock()) {
        let docks = restore_views(&left, &right);
        let mut views = [docks[0].0.clone(), docks[1].0.clone()].concat();
        views.sort_by_key(|&view| view == View::Backlinks);
        prop_assert_eq!(views, [View::Files, View::Backlinks]);

        let entries: Vec<_> = [(Side::Left, &left), (Side::Right, &right)]
            .into_iter()
            .flat_map(|(side, stored)| stored.views.iter().enumerate().map(move |(ix, name)| (side, ix, name)))
            .collect();
        for ((side, stored), (views, active)) in [(Side::Left, &left), (Side::Right, &right)].into_iter().zip(&docks) {
            if views.is_empty() {
                prop_assert_eq!(*active, 0);
            } else {
                prop_assert!(*active < views.len());
            }
            let Some(name) = stored.views.get(stored.active) else { continue };
            let Some(view) = View::named(name) else { continue };
            let first = entries.iter().find(|(_, _, held)| *held == name);
            if first == Some(&(side, stored.active, name)) {
                prop_assert_eq!(views[*active], view);
            }
        }
    }

    #[test]
    fn layout_ops_keep_the_invariants(ops in proptest::collection::vec(op(), 0..64)) {
        let mut layout = Layout::default();
        let center = layout.center().to_vec();
        let mut handed: HashSet<GroupId> = layout.groups().collect();
        for op in ops {
            apply(&mut layout, op, &mut handed)?;
            prop_assert_eq!(layout.center(), &center[..]);
            let mut held = center.clone();
            for side in [Side::Left, Side::Right] {
                prop_assert_eq!(layout.is_drawn(side), layout.is_shown(side) && layout.dock(side).is_some());
                if let Some(group) = layout.dock(side) {
                    prop_assert_eq!(layout.side_of(group), Some(side));
                    held.push(group);
                }
            }
            prop_assert_eq!(layout.groups().collect::<Vec<_>>(), held);
        }
    }

    #[test]
    fn centre_ops_keep_the_row_valid(ops in proptest::collection::vec(centre_op(), 0..48)) {
        let mut layout = Layout::default();
        let mut handed: HashSet<GroupId> = layout.groups().collect();
        for op in ops {
            let before = fractions(&layout);
            let mut changed = Vec::new();
            match op {
                CentreOp::Dock(op) => apply(&mut layout, op, &mut handed)?,
                CentreOp::Split(pick) => {
                    let group = layout.center()[pick % layout.center().len()];
                    let split = layout.split_right(group);
                    prop_assert!(handed.insert(split), "{split:?} handed out twice");
                    changed = vec![group, split];
                }
                CentreOp::Activate(pick) => {
                    let group = layout.center()[pick % layout.center().len()];
                    layout.activate(group);
                    prop_assert_eq!(layout.last_center(), group);
                }
            }
            let flexes = layout.flexes();
            prop_assert_eq!(flexes.len(), layout.center().len());
            prop_assert!(flexes.iter().all(|&flex| flex > 0.0));
            #[allow(clippy::cast_precision_loss)]
            let len = flexes.len() as f32;
            prop_assert!((flexes.iter().sum::<f32>() - len).abs() < 0.001);
            let after = fractions(&layout);
            for (group, fraction) in before {
                if changed.contains(&group) {
                    continue;
                }
                let now = after.iter().find(|(held, _)| *held == group).map(|&(_, now)| now);
                prop_assert!(now.is_some_and(|now| (now - fraction).abs() < 1e-4), "{group:?} went from {fraction} to {now:?}");
            }
            let groups: Vec<_> = layout.groups().collect();
            let unique: HashSet<_> = groups.iter().collect();
            prop_assert_eq!(unique.len(), groups.len());
            prop_assert!(layout.center().contains(&layout.last_center()));
        }
    }
}
