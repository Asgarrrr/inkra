use super::{Dock, Docks, Side, View};

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
