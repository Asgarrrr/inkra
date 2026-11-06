use proptest::prelude::*;

use super::tree::{built, tree_op};
use crate::{Axis, GroupId, Layout, Member, Rect, Side, SplitDirection, Surface};

const DIRECTIONS: [SplitDirection; 4] = [
    SplitDirection::Up,
    SplitDirection::Down,
    SplitDirection::Left,
    SplitDirection::Right,
];

fn opposite(direction: SplitDirection) -> SplitDirection {
    match direction {
        SplitDirection::Up => SplitDirection::Down,
        SplitDirection::Down => SplitDirection::Up,
        SplitDirection::Left => SplitDirection::Right,
        SplitDirection::Right => SplitDirection::Left,
    }
}

fn rect_of(layout: &Layout, group: GroupId) -> Rect {
    let rects = layout.rects(layout.surface_of(group));
    let Some(&(_, rect)) = rects.iter().find(|&&(held, _)| held == group) else {
        panic!("{group:?} has no rect");
    };
    rect
}

/// `rect`'s start and length across `direction`'s axis.
fn span(rect: Rect, direction: SplitDirection) -> (f32, f32) {
    match direction.axis() {
        Axis::Horizontal => (rect.y, rect.height),
        Axis::Vertical => (rect.x, rect.width),
    }
}

/// The point at `probe` along `rect`'s span, kept 1e-4 short of its end: rects are half-open.
fn probed(rect: Rect, direction: SplitDirection, probe: f32) -> f32 {
    let (start, length) = span(rect, direction);
    (start + length * probe).clamp(start, start + (length - 1e-4).max(0.0))
}

fn holds(rect: Rect, direction: SplitDirection, at: f32) -> bool {
    let (start, length) = span(rect, direction);
    start <= at && at < start + length
}

/// `rect`'s edge facing `direction`, along that direction's axis.
fn edge(rect: Rect, direction: SplitDirection) -> f32 {
    match direction {
        SplitDirection::Up => rect.y,
        SplitDirection::Down => rect.y + rect.height,
        SplitDirection::Left => rect.x,
        SplitDirection::Right => rect.x + rect.width,
    }
}

/// `first` and `right` above, `lower_left` and `lower_right` below.
fn two_by_two() -> (Layout, [GroupId; 4]) {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let right = layout.split(first, SplitDirection::Right);
    let lower_left = layout.split(first, SplitDirection::Down);
    let lower_right = layout.split(right, SplitDirection::Down);
    (layout, [first, right, lower_left, lower_right])
}

/// `first` the full height, `right` above `lower` beside it.
fn tall_left() -> (Layout, [GroupId; 3]) {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let right = layout.split(first, SplitDirection::Right);
    let lower = layout.split(right, SplitDirection::Down);
    (layout, [first, right, lower])
}

#[test]
fn left_and_right_run_horizontally_up_and_down_vertically() {
    assert_eq!(SplitDirection::Left.axis(), Axis::Horizontal);
    assert_eq!(SplitDirection::Right.axis(), Axis::Horizontal);
    assert_eq!(SplitDirection::Up.axis(), Axis::Vertical);
    assert_eq!(SplitDirection::Down.axis(), Axis::Vertical);
    assert!(SplitDirection::Left.before());
    assert!(SplitDirection::Up.before());
    assert!(!SplitDirection::Right.before());
    assert!(!SplitDirection::Down.before());
}

#[test]
fn split_left_puts_the_fresh_group_first() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let fresh = layout.split(first, SplitDirection::Left);
    assert_eq!(
        layout.center(),
        &Member::Split {
            axis: Axis::Horizontal,
            children: vec![Member::Group(fresh), Member::Group(first)],
            flexes: vec![1.0, 1.0],
        }
    );
    assert_eq!(layout.last_center(), first);

    let between = layout.split(first, SplitDirection::Left);
    assert_eq!(layout.center_groups(), [fresh, between, first]);
    let Member::Split { flexes, .. } = layout.center() else {
        panic!("the centre is a row");
    };
    assert_eq!(flexes, &[1.5, 0.75, 0.75]);
}

#[test]
fn split_up_puts_the_fresh_group_above() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let fresh = layout.split(first, SplitDirection::Up);
    assert_eq!(
        layout.center(),
        &Member::Split {
            axis: Axis::Vertical,
            children: vec![Member::Group(fresh), Member::Group(first)],
            flexes: vec![1.0, 1.0],
        }
    );
    assert_eq!(layout.top_groups(Surface::Center), [fresh]);
}

#[test]
fn neighbour_crosses_each_edge_of_a_grid() {
    let (layout, [first, right, lower_left, lower_right]) = two_by_two();
    for (from, direction, to) in [
        (first, SplitDirection::Right, right),
        (first, SplitDirection::Down, lower_left),
        (right, SplitDirection::Left, first),
        (right, SplitDirection::Down, lower_right),
        (lower_left, SplitDirection::Up, first),
        (lower_left, SplitDirection::Right, lower_right),
        (lower_right, SplitDirection::Left, lower_left),
        (lower_right, SplitDirection::Up, right),
    ] {
        assert_eq!(
            layout.neighbour(from, direction, 0.5),
            Some(to),
            "{from:?} {direction:?}"
        );
    }
    for (from, direction) in [
        (first, SplitDirection::Up),
        (right, SplitDirection::Up),
        (lower_left, SplitDirection::Down),
        (lower_right, SplitDirection::Down),
    ] {
        assert_eq!(
            layout.neighbour(from, direction, 0.5),
            None,
            "{from:?} {direction:?}"
        );
    }
}

#[test]
fn the_probe_picks_along_the_edge_half_open_and_clamped() {
    let (layout, [first, right, lower]) = tall_left();
    for (probe, to) in [
        (0.2, right),
        (0.49, right),
        (0.5, lower),
        (0.8, lower),
        (-3.0, right),
        (1.0, lower),
        (7.0, lower),
    ] {
        assert_eq!(
            layout.neighbour(first, SplitDirection::Right, probe),
            Some(to),
            "{probe}"
        );
    }

    let mut layout = Layout::default();
    let first = layout.last_center();
    let lower = layout.split(first, SplitDirection::Down);
    let right = layout.split(first, SplitDirection::Right);
    for (probe, to) in [(0.49, first), (0.5, right), (1.0, right)] {
        assert_eq!(
            layout.neighbour(lower, SplitDirection::Up, probe),
            Some(to),
            "{probe}"
        );
    }
}

#[test]
fn neighbour_crosses_into_a_drawn_dock() {
    let (mut layout, [first, right, lower_left, lower_right]) = two_by_two();
    let (files, backlinks) = (GroupId(1), GroupId(2));
    assert_eq!(
        layout.neighbour(first, SplitDirection::Left, 0.5),
        Some(files)
    );
    assert_eq!(
        layout.neighbour(lower_right, SplitDirection::Right, 0.5),
        Some(backlinks)
    );

    let lower_files = layout.split(files, SplitDirection::Down);
    let side_files = layout.split(files, SplitDirection::Right);
    assert_eq!(
        layout.neighbour(first, SplitDirection::Left, 0.5),
        Some(side_files)
    );
    assert_eq!(
        layout.neighbour(lower_left, SplitDirection::Left, 0.5),
        Some(lower_files)
    );
    assert_eq!(
        layout.neighbour(right, SplitDirection::Right, 0.0),
        Some(backlinks)
    );
}

#[test]
fn a_hidden_or_empty_dock_stops_the_neighbour() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    layout.toggle(Side::Left);
    assert_eq!(layout.neighbour(first, SplitDirection::Left, 0.5), None);
    assert_eq!(
        layout.neighbour(first, SplitDirection::Right, 0.5),
        Some(GroupId(2))
    );

    layout.remove(GroupId(2));
    assert_eq!(layout.neighbour(first, SplitDirection::Right, 0.5), None);
}

#[test]
fn a_dock_leads_back_to_last_center() {
    let (mut layout, [_, right, _, lower_right]) = two_by_two();
    let (files, backlinks) = (GroupId(1), GroupId(2));
    layout.activate(lower_right);
    assert_eq!(
        layout.neighbour(files, SplitDirection::Right, 0.1),
        Some(lower_right)
    );
    layout.activate(right);
    assert_eq!(
        layout.neighbour(backlinks, SplitDirection::Left, 0.9),
        Some(right)
    );
    for (from, direction) in [
        (files, SplitDirection::Left),
        (files, SplitDirection::Up),
        (files, SplitDirection::Down),
        (backlinks, SplitDirection::Right),
        (backlinks, SplitDirection::Down),
    ] {
        assert_eq!(
            layout.neighbour(from, direction, 0.5),
            None,
            "{from:?} {direction:?}"
        );
    }

    let lower_files = layout.split(files, SplitDirection::Down);
    assert_eq!(
        layout.neighbour(files, SplitDirection::Down, 0.5),
        Some(lower_files)
    );
    assert_eq!(
        layout.neighbour(lower_files, SplitDirection::Right, 0.5),
        Some(right)
    );
}

proptest! {
    #[test]
    fn a_neighbour_abuts_holds_the_point_and_leads_back(
        ops in proptest::collection::vec(tree_op(), 0..48),
        shown in any::<[bool; 2]>(),
        pick in any::<usize>(),
        direction in proptest::sample::select(DIRECTIONS.to_vec()),
        probe in -0.5f32..1.5,
    ) {
        let mut layout = built(&ops);
        for (side, shown) in [Side::Left, Side::Right].into_iter().zip(shown) {
            if !shown {
                layout.toggle(side);
            }
        }
        let groups: Vec<_> = layout.groups().collect();
        let from = groups[pick % groups.len()];
        let Some(to) = layout.neighbour(from, direction, probe) else {
            return Ok(());
        };
        let (rect, target) = (rect_of(&layout, from), rect_of(&layout, to));
        let at = probed(rect, direction, probe);
        let surfaces = (layout.surface_of(from), layout.surface_of(to));
        if let (Surface::Dock(side), Surface::Center) = surfaces {
            let inner = match side {
                Side::Left => SplitDirection::Right,
                Side::Right => SplitDirection::Left,
            };
            prop_assert_eq!(direction, inner);
            prop_assert_eq!(to, layout.last_center());
            return Ok(());
        }
        prop_assert!(holds(target, direction, at), "{to:?} {target:?} misses {at}");
        let (to_start, to_length) = span(target, direction);
        let back = (at - to_start) / to_length;
        let returned = layout.neighbour(to, opposite(direction), back);
        match surfaces {
            (surface, to_surface) if surface == to_surface => {
                prop_assert!((edge(target, opposite(direction)) - edge(rect, direction)).abs() < 1e-4, "{target:?} misses {rect:?}");
                let again = probed(target, direction, back);
                if holds(rect, direction, again) {
                    prop_assert_eq!(returned, Some(from));
                } else {
                    prop_assert!((again - at).abs() < 2e-4, "the return probes {again}, not {at}");
                    prop_assert!(returned.is_some());
                }
            }
            (Surface::Center, Surface::Dock(side)) => {
                let outer = match side {
                    Side::Left => SplitDirection::Left,
                    Side::Right => SplitDirection::Right,
                };
                prop_assert_eq!(direction, outer);
                prop_assert!(layout.is_drawn(side));
                prop_assert!((edge(rect, direction) - edge(target, opposite(direction))).abs() > 1.0 - 1e-4);
                prop_assert_eq!(returned, Some(layout.last_center()));
            }
            other => prop_assert!(false, "{other:?} crosses between docks"),
        }
    }
}
