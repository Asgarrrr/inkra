use std::collections::HashSet;

use proptest::prelude::*;

use super::{Axis, GroupId, Layout, Member, Side, StoredDock, Surface, View, restore_views};

mod surface;
mod tree;

/// A centre that is one row: its groups left to right and their flexes.
fn row(layout: &Layout) -> (Vec<GroupId>, Vec<f32>) {
    match layout.center() {
        &Member::Group(group) => (vec![group], vec![1.0]),
        Member::Split {
            axis: Axis::Horizontal,
            flexes,
            ..
        } => (layout.center_groups(), flexes.clone()),
        Member::Split { .. } => panic!("the centre is not one row"),
    }
}

#[test]
fn the_default_puts_files_left_and_backlinks_right_both_shown() {
    let layout = Layout::default();
    assert_eq!(View::Files.default_side(), Side::Left);
    assert_eq!(View::Backlinks.default_side(), Side::Right);
    assert_eq!(Side::Left.other(), Side::Right);
    assert_eq!(Side::Right.other(), Side::Left);
    for side in [Side::Left, Side::Right] {
        let [group] = layout.dock_groups(side)[..] else {
            panic!("one group in each dock");
        };
        assert_eq!(layout.surface_of(group), Surface::Dock(side));
        assert!(layout.is_shown(side));
        assert!(layout.is_drawn(side));
    }
    assert_eq!(layout.center(), &Member::Group(GroupId(0)));
    assert_eq!(row(&layout), (vec![GroupId(0)], vec![1.0]));
    assert_eq!(layout.last_center(), GroupId(0));
    assert_eq!(layout.surface_of(GroupId(0)), Surface::Center);
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
    let left = layout.dock_groups(Side::Left)[0];
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
    let right = layout.dock_groups(Side::Right)[0];
    layout.remove(right);
    let added = layout.add_dock(Side::Right);
    assert!(![GroupId(0), GroupId(1), right].contains(&added));
    assert_eq!(layout.dock(Side::Right), Some(&Member::Group(added)));
    assert_eq!(layout.surface_of(added), Surface::Dock(Side::Right));
    assert!(layout.is_drawn(Side::Right));
}

#[test]
fn removing_a_dock_group_keeps_its_flag() {
    let mut layout = Layout::default();
    layout.toggle(Side::Right);
    let right = layout.dock_groups(Side::Right)[0];
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
    layout.remove(layout.center_groups()[0]);
}

#[test]
fn split_right_puts_the_new_group_after_and_halves_its_room() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout.split(first, Axis::Horizontal);
    assert_eq!(row(&layout).0, [first, second]);
    assert_eq!(row(&layout).1, [1.0, 1.0]);
    assert_eq!(layout.last_center(), first);
    assert_eq!(layout.surface_of(second), Surface::Center);

    let third = layout.split(first, Axis::Horizontal);
    assert_eq!(row(&layout).0, [first, third, second]);
    assert_eq!(row(&layout).1, [0.75, 0.75, 1.5]);
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
    let second = layout.split(first, Axis::Horizontal);
    layout.split(second, Axis::Horizontal);
    assert_eq!(row(&layout).1, [1.5, 0.75, 0.75]);
}

#[test]
fn removing_a_dock_group_returns_no_heir() {
    let mut layout = Layout::default();
    let left = layout.dock_groups(Side::Left)[0];
    assert_eq!(layout.remove(left), None);
}

#[test]
fn removing_a_centre_group_gives_its_room_to_the_left() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let third = layout.split(first, Axis::Horizontal);
    let second = layout.split(first, Axis::Horizontal);
    assert_eq!(row(&layout).1, [0.75, 0.75, 1.5]);

    assert_eq!(layout.remove(second), Some(first));

    assert_eq!(row(&layout).0, [first, third]);
    assert_eq!(row(&layout).1, [1.0, 1.0]);
    assert_eq!(layout.last_center(), first);
    assert_eq!(
        layout.groups().collect::<Vec<_>>(),
        [first, third, GroupId(1), GroupId(2)]
    );
}

#[test]
fn the_first_group_gives_its_room_to_the_right() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let third = layout.split(first, Axis::Horizontal);
    let second = layout.split(first, Axis::Horizontal);

    assert_eq!(layout.remove(first), Some(second));

    assert_eq!(row(&layout).0, [second, third]);
    assert_eq!(row(&layout).1, [1.0, 1.0]);
}

#[test]
fn removing_the_last_center_moves_it_to_the_heir() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout.split(first, Axis::Horizontal);
    let third = layout.split(second, Axis::Horizontal);
    layout.activate(second);

    assert_eq!(layout.remove(second), Some(first));

    assert_eq!(layout.last_center(), first);
    layout.activate(third);
    assert_eq!(layout.remove(first), Some(third));
    assert_eq!(layout.last_center(), third);
}

#[test]
#[should_panic(expected = "the centre always has a group")]
fn removing_the_centres_only_group_panics() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout.split(first, Axis::Horizontal);
    layout.remove(first);
    layout.remove(second);
}

#[test]
#[should_panic(expected = "is not in the centre")]
fn activating_a_dock_group_panics() {
    let mut layout = Layout::default();
    layout.activate(GroupId(1));
}

#[test]
#[should_panic(expected = "is not in the layout")]
fn splitting_an_absent_group_panics() {
    let mut layout = Layout::default();
    layout.split(GroupId(7), Axis::Horizontal);
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

/// `restore_views` on one group a side.
fn restore_pair(left: &StoredDock, right: &StoredDock) -> [(Vec<View>, usize); 2] {
    restore_views([std::slice::from_ref(left), std::slice::from_ref(right)]).map(|groups| {
        let [group] = <[_; 1]>::try_from(groups).expect("one group a side");
        group
    })
}

#[test]
fn a_view_in_both_docks_is_kept_left() {
    let views = restore_pair(
        &state(&[View::Files, View::Backlinks], 1),
        &state(&[View::Backlinks], 0),
    );
    assert_eq!(views, both_left(1));
}

#[test]
fn a_view_twice_in_one_dock_is_kept_first() {
    let views = restore_pair(
        &state(&[View::Files, View::Files], 0),
        &state(&[View::Backlinks], 0),
    );
    assert_eq!(views, defaults());
}

#[test]
fn a_missing_view_is_appended_to_its_default_side() {
    let views = restore_pair(&state(&[], 0), &state(&[View::Backlinks], 0));
    assert_eq!(views, defaults());

    let views = restore_pair(&state(&[View::Files], 0), &state(&[], 0));
    assert_eq!(views, defaults());

    let views = restore_pair(&state(&[], 0), &state(&[View::Files], 0));
    assert_eq!(
        views,
        [(Vec::new(), 0), (vec![View::Files, View::Backlinks], 0)]
    );
}

#[test]
fn an_active_index_out_of_range_is_clamped() {
    let views = restore_pair(&state(&[View::Files, View::Backlinks], 7), &state(&[], 3));
    assert_eq!(views, both_left(1));
}

#[test]
fn both_docks_empty_restore_the_default_views_with_their_flags() {
    let views = restore_pair(&state(&[], 0), &state(&[], 0));
    assert_eq!(views, defaults());
}

fn stored(names: &[&str], active: usize) -> StoredDock {
    StoredDock {
        views: names.iter().map(|&name| name.to_owned()).collect(),
        active,
    }
}

#[test]
fn an_unknown_name_before_the_active_keeps_the_active_view() {
    let [(views, active), _] = restore_pair(
        &stored(&["graph", "files", "backlinks"], 1),
        &stored(&[], 0),
    );
    assert_eq!(views[active], View::Files);
}

#[test]
fn an_active_duplicate_maps_to_its_kept_view() {
    let [(views, active), _] = restore_pair(
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
    (proptest::collection::vec(name(), 0..6), 0..8usize)
        .prop_map(|(views, active)| StoredDock { views, active })
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
    Remove(usize),
}

fn centre_op() -> impl Strategy<Value = CentreOp> {
    prop_oneof![
        op().prop_map(CentreOp::Dock),
        any::<usize>().prop_map(CentreOp::Split),
        any::<usize>().prop_map(CentreOp::Activate),
        any::<usize>().prop_map(CentreOp::Remove),
    ]
}

fn fractions(layout: &Layout) -> Vec<(GroupId, f32)> {
    let (center, flexes) = row(layout);
    let sum: f32 = flexes.iter().sum();
    center
        .into_iter()
        .zip(flexes)
        .map(|(group, flex)| (group, flex / sum))
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
            if let Some(&group) = layout.dock_groups(side).first() {
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
        let docks = restore_pair(&left, &right);
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
        let center = layout.center_groups();
        let mut handed: HashSet<GroupId> = layout.groups().collect();
        for op in ops {
            apply(&mut layout, op, &mut handed)?;
            prop_assert_eq!(layout.center_groups(), &center[..]);
            let mut held = center.clone();
            for side in [Side::Left, Side::Right] {
                prop_assert_eq!(layout.is_drawn(side), layout.is_shown(side) && layout.dock(side).is_some());
                for group in layout.dock_groups(side) {
                    prop_assert_eq!(layout.surface_of(group), Surface::Dock(side));
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
                    let center = layout.center_groups();
                    let group = center[pick % center.len()];
                    let split = layout.split(group, Axis::Horizontal);
                    prop_assert!(handed.insert(split), "{split:?} handed out twice");
                    changed = vec![group, split];
                }
                CentreOp::Activate(pick) => {
                    let center = layout.center_groups();
                    let group = center[pick % center.len()];
                    layout.activate(group);
                    prop_assert_eq!(layout.last_center(), group);
                }
                CentreOp::Remove(pick) => {
                    let center = layout.center_groups();
                    if center.len() == 1 {
                        continue;
                    }
                    let ix = pick % center.len();
                    let was_last = layout.last_center() == center[ix];
                    let heir = layout.remove(center[ix]).expect("a centre group has an heir");
                    prop_assert_eq!(heir, center[ix.checked_sub(1).unwrap_or(1)]);
                    prop_assert!(!layout.center_groups().contains(&center[ix]));
                    if was_last {
                        prop_assert_eq!(layout.last_center(), heir);
                    }
                    let fraction = |group| before.iter().find(|(held, _)| *held == group).map_or(0.0, |&(_, fraction)| fraction);
                    let now = fractions(&layout).into_iter().find(|(held, _)| *held == heir).map(|(_, now)| now);
                    let joined = fraction(heir) + fraction(center[ix]);
                    prop_assert!(now.is_some_and(|now| (now - joined).abs() < 1e-4), "{heir:?} has {now:?}, not {joined}");
                    changed = vec![heir, center[ix]];
                }
            }
            let (center, flexes) = row(&layout);
            prop_assert_eq!(flexes.len(), center.len());
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
            prop_assert!(layout.center_groups().contains(&layout.last_center()));
        }
    }
}
