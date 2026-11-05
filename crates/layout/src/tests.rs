use proptest::prelude::*;

use super::{Dock, Docks, Side, StoredDock, View};

#[test]
fn the_default_puts_files_left_and_backlinks_right_both_shown() {
    let docks = Docks::default();
    let left = docks.dock(Side::Left);
    let right = docks.dock(Side::Right);
    assert_eq!(left.views(), [View::Files]);
    assert_eq!(left.active(), Some(View::Files));
    assert!(left.is_shown());
    assert_eq!(right.views(), [View::Backlinks]);
    assert_eq!(right.active(), Some(View::Backlinks));
    assert!(right.is_shown());
}

#[test]
fn toggle_flips_only_its_side() {
    let mut docks = Docks::default();
    docks.toggle(Side::Left);
    assert!(!docks.dock(Side::Left).is_shown());
    assert!(docks.dock(Side::Right).is_shown());
    docks.toggle(Side::Right);
    assert!(!docks.dock(Side::Left).is_shown());
    assert!(!docks.dock(Side::Right).is_shown());
}

#[test]
fn toggling_twice_restores_the_layout() {
    let mut docks = Docks::default();
    docks.toggle(Side::Right);
    docks.toggle(Side::Right);
    assert_eq!(docks, Docks::default());
}

#[test]
fn reveal_shows_a_hidden_dock_and_activates_the_view() {
    let mut docks = Docks {
        left: Dock {
            views: vec![View::Files, View::Backlinks],
            active: 0,
            shown: false,
        },
        right: Dock {
            views: Vec::new(),
            active: 0,
            shown: true,
        },
    };
    docks.reveal(View::Backlinks);
    assert!(docks.dock(Side::Left).is_shown());
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Backlinks));
    assert!(docks.is_visible(View::Backlinks));
    assert!(!docks.is_visible(View::Files));
}

#[test]
fn reveal_leaves_the_other_dock_alone() {
    let mut docks = Docks::default();
    docks.toggle(Side::Right);
    let hidden = docks.clone();
    docks.reveal(View::Files);
    assert_eq!(docks, hidden);
    docks.reveal(View::Backlinks);
    assert_eq!(docks, Docks::default());
}

#[test]
fn a_hidden_dock_shows_no_visible_view() {
    let mut docks = Docks::default();
    assert!(docks.is_visible(View::Files));
    assert!(docks.is_visible(View::Backlinks));
    docks.toggle(Side::Left);
    assert!(!docks.is_visible(View::Files));
    assert!(docks.is_visible(View::Backlinks));
}

/// Both views in the left dock, `active` the active one's index; the right dock empty and shown.
fn both_left(active: usize) -> Docks {
    Docks {
        left: Dock {
            views: vec![View::Files, View::Backlinks],
            active,
            shown: true,
        },
        right: Dock {
            views: Vec::new(),
            active: 0,
            shown: true,
        },
    }
}

#[test]
fn a_moved_view_becomes_active_and_its_dock_shows() {
    let mut docks = Docks::default();
    docks.toggle(Side::Right);
    docks.move_view(View::Files, Side::Right, 1);
    let right = docks.dock(Side::Right);
    assert_eq!(right.views(), [View::Backlinks, View::Files]);
    assert_eq!(right.active(), Some(View::Files));
    assert!(right.is_shown());
    assert_eq!(docks.side_of(View::Files), Side::Right);
    assert!(docks.is_visible(View::Files));
}

#[test]
fn the_right_neighbour_takes_the_active_slot() {
    let mut docks = both_left(0);
    docks.move_view(View::Files, Side::Right, 0);
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Backlinks));
}

#[test]
fn the_left_neighbour_takes_it_when_the_active_was_last() {
    let mut docks = both_left(1);
    docks.move_view(View::Backlinks, Side::Right, 0);
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Files));
}

#[test]
fn removing_a_view_before_the_active_keeps_the_active_view() {
    let mut docks = both_left(1);
    docks.move_view(View::Files, Side::Right, 0);
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Backlinks));
}

#[test]
fn a_move_inside_a_dock_reorders() {
    let mut docks = both_left(0);
    docks.move_view(View::Backlinks, Side::Left, 0);
    let left = docks.dock(Side::Left);
    assert_eq!(left.views(), [View::Backlinks, View::Files]);
    assert_eq!(left.active(), Some(View::Backlinks));
    assert!(docks.dock(Side::Right).views().is_empty());
}

#[test]
fn an_empty_dock_is_not_drawn_and_keeps_its_flag() {
    let mut docks = Docks::default();
    assert!(docks.dock(Side::Left).is_drawn());
    docks.move_view(View::Files, Side::Right, 0);
    let left = docks.dock(Side::Left);
    assert!(left.is_shown());
    assert!(!left.is_drawn());
    assert_eq!(left.active(), None);

    docks.toggle(Side::Left);
    assert!(!docks.dock(Side::Left).is_shown());
    docks.toggle(Side::Right);
    assert!(!docks.dock(Side::Right).is_drawn());
}

#[test]
#[should_panic(expected = "past the end")]
fn an_index_past_the_end_panics() {
    Docks::default().move_view(View::Files, Side::Right, 2);
}

#[test]
#[should_panic(expected = "past the end")]
fn an_index_past_the_end_of_its_own_dock_panics() {
    Docks::default().move_view(View::Files, Side::Left, 1);
}

#[test]
fn append_on_the_same_side_moves_to_the_end() {
    let mut docks = Docks::default();
    docks.move_view(View::Backlinks, Side::Left, 1);
    docks.append(View::Files, Side::Left);
    let left = docks.dock(Side::Left);
    assert_eq!(left.views(), [View::Backlinks, View::Files]);
    assert_eq!(left.active(), Some(View::Files));
}

#[test]
fn append_on_the_other_side_moves_to_the_end() {
    let mut docks = Docks::default();
    docks.append(View::Files, Side::Right);
    let right = docks.dock(Side::Right);
    assert_eq!(right.views(), [View::Backlinks, View::Files]);
    assert_eq!(right.active(), Some(View::Files));
    assert!(right.is_shown());
    assert!(docks.dock(Side::Left).views().is_empty());
}

fn state(views: &[View], active: usize, shown: bool) -> StoredDock {
    StoredDock {
        views: views.iter().map(|view| view.name().to_owned()).collect(),
        active,
        shown,
    }
}

#[test]
fn a_view_in_both_docks_is_kept_left() {
    let docks = Docks::restore(
        state(&[View::Files, View::Backlinks], 1, true),
        state(&[View::Backlinks], 0, true),
    );
    assert_eq!(docks, both_left(1));
}

#[test]
fn a_view_twice_in_one_dock_is_kept_first() {
    let docks = Docks::restore(
        state(&[View::Files, View::Files], 0, true),
        state(&[View::Backlinks], 0, true),
    );
    assert_eq!(docks, Docks::default());
}

#[test]
fn a_missing_view_is_appended_to_its_default_side() {
    let docks = Docks::restore(state(&[], 0, true), state(&[View::Backlinks], 0, false));
    assert_eq!(docks.dock(Side::Left).views(), [View::Files]);
    assert_eq!(docks.dock(Side::Right).views(), [View::Backlinks]);
    assert!(!docks.dock(Side::Right).is_shown());

    let docks = Docks::restore(state(&[View::Files], 0, true), state(&[], 0, true));
    assert_eq!(docks, Docks::default());
}

#[test]
fn an_active_index_out_of_range_is_clamped() {
    let docks = Docks::restore(
        state(&[View::Files, View::Backlinks], 7, true),
        state(&[], 3, true),
    );
    assert_eq!(docks, both_left(1));
}

#[test]
fn both_docks_empty_restore_the_default_views_with_their_flags() {
    let docks = Docks::restore(state(&[], 0, false), state(&[], 0, true));
    let mut expected = Docks::default();
    expected.toggle(Side::Left);
    assert_eq!(docks, expected);
}

#[test]
fn state_reads_each_dock() {
    let docks = both_left(1);
    assert_eq!(
        docks.state(Side::Left),
        state(&[View::Files, View::Backlinks], 1, true)
    );
    assert_eq!(docks.state(Side::Right), state(&[], 0, true));
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
    let docks = Docks::restore(stored(&["graph", "files", "backlinks"], 1), stored(&[], 0));
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Files));
}

#[test]
fn an_active_duplicate_maps_to_its_kept_view() {
    let docks = Docks::restore(stored(&["files", "files", "backlinks"], 1), stored(&[], 0));
    assert_eq!(docks.dock(Side::Left).active(), Some(View::Files));
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

#[derive(Debug, Clone, Copy)]
enum Op {
    Toggle(Side),
    Reveal(View),
    Move(View, Side, u8),
    Append(View, Side),
}

fn side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Left), Just(Side::Right)]
}

fn view() -> impl Strategy<Value = View> {
    prop_oneof![Just(View::Files), Just(View::Backlinks)]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        side().prop_map(Op::Toggle),
        view().prop_map(Op::Reveal),
        (view(), side(), any::<u8>()).prop_map(|(view, side, seed)| Op::Move(view, side, seed)),
        (view(), side()).prop_map(|(view, side)| Op::Append(view, side)),
    ]
}

/// Applies `op` and checks the rule it promises.
fn apply(docks: &mut Docks, op: Op) -> Result<(), TestCaseError> {
    match op {
        Op::Toggle(side) => docks.toggle(side),
        Op::Reveal(view) => {
            docks.reveal(view);
            prop_assert!(docks.is_visible(view));
        }
        Op::Move(view, side, seed) => {
            let len = docks.dock(side).views().len();
            let len = if docks.side_of(view) == side {
                len - 1
            } else {
                len
            };
            docks.move_view(view, side, usize::from(seed) % (len + 1));
            prop_assert_eq!(docks.side_of(view), side);
            prop_assert_eq!(docks.dock(side).active(), Some(view));
            prop_assert!(docks.dock(side).is_shown());
        }
        Op::Append(view, side) => {
            docks.append(view, side);
            prop_assert_eq!(docks.dock(side).views().last(), Some(&view));
            prop_assert_eq!(docks.dock(side).active(), Some(view));
            prop_assert!(docks.dock(side).is_shown());
        }
    }
    Ok(())
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

proptest! {
    #[test]
    fn restore_repairs_any_stored_names(left in stored_dock(), right in stored_dock()) {
        let docks = Docks::restore(left.clone(), right.clone());
        let mut views = [docks.dock(Side::Left).views(), docks.dock(Side::Right).views()].concat();
        views.sort_by_key(|&view| view == View::Backlinks);
        prop_assert_eq!(views, [View::Files, View::Backlinks]);

        let entries: Vec<_> = [(Side::Left, &left), (Side::Right, &right)]
            .into_iter()
            .flat_map(|(side, stored)| stored.views.iter().enumerate().map(move |(ix, name)| (side, ix, name)))
            .collect();
        for (side, stored) in [(Side::Left, &left), (Side::Right, &right)] {
            let dock = docks.dock(side);
            prop_assert_eq!(dock.active().is_some(), !dock.views().is_empty());
            let Some(name) = stored.views.get(stored.active) else { continue };
            let Some(view) = View::named(name) else { continue };
            let first = entries.iter().find(|(_, _, held)| *held == name);
            if first == Some(&(side, stored.active, name)) {
                prop_assert_eq!(dock.active(), Some(view));
            }
        }
    }

    #[test]
    fn restore_round_trips_any_layout(ops in proptest::collection::vec(op(), 0..64)) {
        let mut docks = Docks::default();
        for op in ops {
            apply(&mut docks, op)?;
        }
        prop_assert_eq!(Docks::restore(docks.state(Side::Left), docks.state(Side::Right)), docks);
    }

    #[test]
    fn every_op_keeps_each_view_in_exactly_one_dock(ops in proptest::collection::vec(op(), 0..64)) {
        let mut docks = Docks::default();
        for op in ops {
            apply(&mut docks, op)?;
            let mut views = [docks.dock(Side::Left).views(), docks.dock(Side::Right).views()].concat();
            views.sort_by_key(|&view| view == View::Backlinks);
            prop_assert_eq!(views, [View::Files, View::Backlinks]);
            for side in [Side::Left, Side::Right] {
                let dock = docks.dock(side);
                prop_assert_eq!(dock.active().is_some(), !dock.views().is_empty());
            }
        }
    }
}
