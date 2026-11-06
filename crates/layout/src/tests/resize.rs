use proptest::prelude::*;

use super::tree::{built, tree_op};
use crate::resize::minimum;
use crate::{Axis, GroupId, Layout, Member, Shape, Side, SplitPath, Surface};

fn path(surface: Surface, steps: &[usize]) -> SplitPath {
    SplitPath {
        surface,
        steps: steps.to_vec(),
    }
}

fn root() -> SplitPath {
    path(Surface::Center, &[])
}

fn flexes(layout: &Layout, path: &SplitPath) -> Vec<f32> {
    match layout.member_at(path) {
        Some(Member::Split { flexes, .. }) => flexes.clone(),
        other => panic!("no split at {path:?}: {other:?}"),
    }
}

fn near(was: &[f32], now: &[f32]) -> bool {
    was.len() == now.len() && was.iter().zip(now).all(|(a, b)| (a - b).abs() < 1e-4)
}

fn shape_split(axis: Axis, children: Vec<Shape>, flexes: Vec<f32>) -> Shape {
    Shape::Split {
        axis,
        children,
        flexes,
    }
}

/// A centre row of `flexes.len()` groups, ids from 0, no docks.
fn row(flexes: &[f32]) -> Layout {
    let center = shape_split(
        Axis::Horizontal,
        vec![Shape::Group; flexes.len()],
        flexes.to_vec(),
    );
    Layout::restored(&center, [None, None], [true, true], 0)
}

/// `g0 | (g1 g2 over g3)`: the right child's minimum width is the larger of its rows'.
fn nested() -> Layout {
    let row = shape_split(
        Axis::Horizontal,
        vec![Shape::Group, Shape::Group],
        vec![1.0, 1.0],
    );
    let column = shape_split(Axis::Vertical, vec![row, Shape::Group], vec![1.0, 1.0]);
    let center = shape_split(Axis::Horizontal, vec![Shape::Group, column], vec![1.0, 1.0]);
    Layout::restored(&center, [None, None], [true, true], 0)
}

/// Group `n`'s minimum is `10 * (n + 1)` wide and `1000 + n` high.
fn by_id(group: GroupId, axis: Axis) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let n = group.raw() as f32;
    match axis {
        Axis::Horizontal => 10.0 * (n + 1.0),
        Axis::Vertical => 1000.0 + n,
    }
}

fn flat(min: f32) -> impl Fn(GroupId, Axis) -> f32 {
    move |_, _| min
}

#[test]
fn a_drag_moves_the_boundary_between_its_two_neighbours() {
    let mut layout = row(&[1.0, 1.0, 1.0]);

    assert!(layout.resize(&root(), 1, 30.0, 300.0, flat(10.0)));

    assert!(near(&flexes(&layout, &root()), &[1.3, 0.7, 1.0]));
    assert!(layout.resize(&root(), 2, -50.0, 300.0, flat(10.0)));
    assert!(near(&flexes(&layout, &root()), &[1.3, 0.2, 1.5]));
    assert_eq!(layout.center_groups(), [GroupId(0), GroupId(1), GroupId(2)]);
}

#[test]
fn a_drag_stops_at_either_neighbours_minimum() {
    let mut layout = row(&[1.0, 1.0, 1.0]);
    assert!(layout.resize(&root(), 1, -1000.0, 300.0, flat(40.0)));
    assert!(near(&flexes(&layout, &root()), &[0.4, 1.6, 1.0]));

    let mut layout = row(&[1.0, 1.0, 1.0]);
    assert!(layout.resize(&root(), 1, 1000.0, 300.0, flat(40.0)));
    assert!(near(&flexes(&layout, &root()), &[1.6, 0.4, 1.0]));

    assert!(!layout.resize(&root(), 1, 5.0, 300.0, flat(40.0)));
    assert!(near(&flexes(&layout, &root()), &[1.6, 0.4, 1.0]));
}

#[test]
fn an_undersized_side_may_grow_but_is_not_forced_bigger() {
    let mut layout = row(&[0.2, 1.8]);

    assert!(!layout.resize(&root(), 1, -5.0, 200.0, flat(80.0)));
    assert!(near(&flexes(&layout, &root()), &[0.2, 1.8]));

    assert!(layout.resize(&root(), 1, 5.0, 200.0, flat(80.0)));
    assert!(near(&flexes(&layout, &root()), &[0.25, 1.75]));
}

#[test]
fn a_split_child_takes_its_minimum_from_its_groups() {
    let layout = nested();
    let Some(Member::Split { children, .. }) = layout.member_at(&root()) else {
        panic!("the centre is a split");
    };
    let column = &children[1];
    let Member::Split { children: rows, .. } = column else {
        panic!("the right child is a split");
    };

    // Widths: the row sums g1 and g2, 20 + 30; g3's 40 is less.
    assert!((minimum(&rows[0], Axis::Horizontal, &by_id) - 50.0).abs() < 1e-4);
    assert!((minimum(column, Axis::Horizontal, &by_id) - 50.0).abs() < 1e-4);
    // Heights: the row is its taller group, g2's 1002; the column adds g3's 1003.
    assert!((minimum(&rows[0], Axis::Vertical, &by_id) - 1002.0).abs() < 1e-4);
    assert!((minimum(column, Axis::Vertical, &by_id) - 2005.0).abs() < 1e-4);
    let min = |group: GroupId, axis| {
        if group == GroupId(3) {
            70.0
        } else {
            by_id(group, axis)
        }
    };
    assert!((minimum(column, Axis::Horizontal, &min) - 70.0).abs() < 1e-4);
}

#[test]
fn a_drag_against_a_nested_split_stops_at_its_minimum() {
    let mut layout = nested();

    assert!(layout.resize(&root(), 1, 1000.0, 600.0, by_id));

    assert!(near(&flexes(&layout, &root()), &[1.8333, 0.1667]));
    let column = path(Surface::Center, &[1]);
    assert!(layout.resize(&column, 1, -1000.0, 2500.0, by_id));
    assert!(near(&flexes(&layout, &column), &[0.8016, 1.1984]));
}

#[test]
fn a_drag_in_a_dock_resizes_its_groups() {
    let mut layout = Layout::default();
    let files = layout.dock_groups(Side::Left)[0];
    layout.split(files, Axis::Vertical);
    let dock = path(Surface::Dock(Side::Left), &[]);

    assert!(layout.resize(&dock, 1, 100.0, 1000.0, flat(100.0)));

    assert!(near(&flexes(&layout, &dock), &[1.2, 0.8]));
    assert_eq!(layout.center(), &Member::Group(GroupId(0)));
}

#[test]
fn a_drag_that_names_no_boundary_changes_nothing() {
    let layout = row(&[1.0, 1.0]);
    let refused = [
        (path(Surface::Center, &[5]), 1, 10.0, 200.0),
        (path(Surface::Center, &[0]), 1, 10.0, 200.0),
        (path(Surface::Dock(Side::Left), &[]), 1, 10.0, 200.0),
        (root(), 0, 10.0, 200.0),
        (root(), 2, 10.0, 200.0),
        (root(), 1, 10.0, 0.0),
        (root(), 1, 10.0, -5.0),
        (root(), 1, 0.05, 200.0),
        (root(), 1, 0.0, 200.0),
    ];
    for (path, ix, delta, size) in refused {
        let mut resized = layout.clone();
        assert!(
            !resized.resize(&path, ix, delta, size, flat(10.0)),
            "{path:?} {ix} {delta} {size}"
        );
        assert_eq!(resized, layout);
    }
    let mut resized = layout.clone();
    assert!(resized.resize(&root(), 1, 0.2, 200.0, flat(10.0)));
}

#[test]
fn a_change_of_exactly_epsilon_is_kept() {
    let a = 1.0 / 512.0;
    let mut layout = row(&[a, 2.0 - a]);

    assert!(layout.resize(&root(), 1, -0.001, 2.0, flat(0.0)));

    assert!(near(
        &flexes(&layout, &root()),
        &[a - 0.001, 2.0 - a + 0.001]
    ));
}

#[test]
fn equalize_resets_every_flex_to_one() {
    let mut layout = row(&[1.5, 0.25, 1.25]);

    assert!(layout.equalize(&root()));

    assert_eq!(flexes(&layout, &root()), [1.0, 1.0, 1.0]);
    assert!(!layout.equalize(&root()));
    let mut close = row(&[1.0005, 0.9995]);
    assert!(close.equalize(&root()));
    assert!(!layout.equalize(&path(Surface::Center, &[0])));
    assert!(!layout.equalize(&path(Surface::Center, &[3])));
    assert!(!layout.equalize(&path(Surface::Dock(Side::Right), &[])));
    let mut nested = nested();
    assert!(nested.resize(&root(), 1, 30.0, 600.0, flat(10.0)));
    assert!(nested.equalize(&root()));
    assert_eq!(nested, self::nested());
}

#[test]
fn member_at_walks_the_steps() {
    let layout = nested();
    assert_eq!(
        layout.member_at(&path(Surface::Center, &[1, 1])),
        Some(&Member::Group(GroupId(3)))
    );
    assert_eq!(
        layout.member_at(&path(Surface::Center, &[0])),
        Some(&Member::Group(GroupId(0)))
    );
    assert_eq!(layout.member_at(&path(Surface::Center, &[0, 0])), None);
    assert_eq!(layout.member_at(&path(Surface::Center, &[2])), None);
    assert_eq!(
        layout.member_at(&path(Surface::Dock(Side::Left), &[])),
        None
    );
    assert_eq!(
        Layout::default().member_at(&path(Surface::Dock(Side::Right), &[])),
        Some(&Member::Group(GroupId(2)))
    );
}

fn collect_splits(member: &Member, at: &mut SplitPath, splits: &mut Vec<SplitPath>) {
    if let Member::Split { children, .. } = member {
        splits.push(at.clone());
        for (ix, child) in children.iter().enumerate() {
            at.steps.push(ix);
            collect_splits(child, at, splits);
            at.steps.pop();
        }
    }
}

fn all_splits(layout: &Layout) -> Vec<SplitPath> {
    let mut splits = Vec::new();
    for surface in [
        Surface::Center,
        Surface::Dock(Side::Left),
        Surface::Dock(Side::Right),
    ] {
        let mut at = path(surface, &[]);
        if let Some(member) = layout.member_at(&at) {
            collect_splits(member, &mut at, &mut splits);
        }
    }
    splits
}

proptest! {
    #[test]
    fn a_drag_keeps_the_sum_and_each_minimum(
        ops in proptest::collection::vec(tree_op(), 0..48),
        pick in any::<usize>(),
        ix in any::<usize>(),
        delta in -3000f32..3000.0,
        size in 1f32..3000.0,
        mins in proptest::collection::vec(1f32..400.0, 1..8),
    ) {
        let mut layout = built(&ops);
        let splits = all_splits(&layout);
        prop_assume!(!splits.is_empty());
        let path = &splits[pick % splits.len()];
        let before = layout.clone();
        let Some(Member::Split { axis, children, flexes: was }) = before.member_at(path) else {
            unreachable!("a collected path names a split");
        };
        let len = children.len();
        let ix = 1 + ix % (len - 1);
        let min = |group: GroupId, axis: Axis| {
            let axis = usize::from(axis == Axis::Vertical);
            mins[(group.raw() as usize * 2 + axis) % mins.len()]
        };

        let changed = layout.resize(path, ix, delta, size, min);

        let now = flexes(&layout, path);
        #[allow(clippy::cast_precision_loss)]
        let count = len as f32;
        prop_assert!((now.iter().sum::<f32>() - count).abs() < 1e-3, "{now:?}");
        for ((child, was), now) in children.iter().zip(was).zip(&now) {
            let (was, now) = (was * size / count, now * size / count);
            let floor = minimum(child, *axis, &min).min(was);
            prop_assert!(now >= floor - 1e-2, "{now} under {floor}, was {was}");
        }
        prop_assert!(layout.groups().eq(before.groups()));
        if changed {
            let moved: Vec<_> = (0..len).filter(|&j| (now[j] - was[j]).abs() > 1e-6).collect();
            prop_assert!(moved.iter().all(|&j| j == ix - 1 || j == ix), "{moved:?}");
        } else {
            prop_assert_eq!(&layout, &before);
        }
    }
}
