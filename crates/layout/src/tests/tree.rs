use std::collections::HashSet;

use proptest::prelude::*;

use super::no_min;
use crate::tree::{normalised, rects};
use crate::{Axis, GroupId, Layout, Member, Rect, Shape, Side, SplitDirection, Surface};

fn group(raw: u32) -> Member {
    Member::Group(GroupId(raw))
}

fn split(axis: Axis, children: Vec<Member>, flexes: Vec<f32>) -> Member {
    Member::Split {
        axis,
        children,
        flexes,
    }
}

/// `member` as a session stores it: the tree without its ids.
fn shape(member: &Member) -> Shape {
    match member {
        Member::Group(_) => Shape::Group,
        Member::Split {
            axis,
            children,
            flexes,
        } => Shape::Split {
            axis: *axis,
            children: children.iter().map(shape).collect(),
            flexes: flexes.clone(),
        },
    }
}

fn shape_split(axis: Axis, children: Vec<Shape>, flexes: Vec<f32>) -> Shape {
    Shape::Split {
        axis,
        children,
        flexes,
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// `first`, then `second` right of it, then `third` below `second`.
fn grid() -> (Layout, [GroupId; 3]) {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout
        .split(first, SplitDirection::Right, 1.0, no_min)
        .unwrap();
    let third = layout
        .split(second, SplitDirection::Down, 1.0, no_min)
        .unwrap();
    (layout, [first, second, third])
}

#[test]
fn split_down_stacks_the_new_group_below() {
    let mut layout = Layout::default();
    let first = layout.last_center();

    let second = layout
        .split(first, SplitDirection::Down, 1.0, no_min)
        .unwrap();

    assert_eq!(
        layout.center(),
        &split(
            Axis::Vertical,
            vec![Member::Group(first), Member::Group(second)],
            vec![1.0, 1.0]
        )
    );
    assert_eq!(layout.center_groups(), [first, second]);
    assert_eq!(layout.last_center(), first);
    assert_eq!(layout.surface_of(second), Surface::Center);
    assert_eq!(
        layout.rects(Surface::Center),
        [
            (first, rect(0.0, 0.0, 1.0, 0.5)),
            (second, rect(0.0, 0.5, 1.0, 0.5))
        ]
    );

    let (layout, [first, second, third]) = grid();
    assert_eq!(
        layout.center(),
        &split(
            Axis::Horizontal,
            vec![
                Member::Group(first),
                split(
                    Axis::Vertical,
                    vec![Member::Group(second), Member::Group(third)],
                    vec![1.0, 1.0]
                )
            ],
            vec![1.0, 1.0]
        )
    );
}

#[test]
fn a_same_axis_split_inserts_a_sibling() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout
        .split(first, SplitDirection::Down, 1.0, no_min)
        .unwrap();

    let third = layout
        .split(first, SplitDirection::Down, 1.0, no_min)
        .unwrap();

    assert_eq!(
        layout.center(),
        &split(
            Axis::Vertical,
            vec![
                Member::Group(first),
                Member::Group(third),
                Member::Group(second)
            ],
            vec![0.75, 0.75, 1.5]
        )
    );
    assert_eq!(layout.center_groups(), [first, third, second]);
    assert_eq!(layout.last_center(), first);
}

#[test]
fn a_one_child_split_collapses() {
    assert_eq!(
        normalised(split(Axis::Horizontal, vec![group(0)], vec![1.0])),
        group(0)
    );
    assert_eq!(
        normalised(split(
            Axis::Vertical,
            vec![split(Axis::Horizontal, vec![group(1)], vec![1.0]), group(2)],
            vec![1.0, 1.0]
        )),
        split(Axis::Vertical, vec![group(1), group(2)], vec![1.0, 1.0])
    );
}

#[test]
fn a_nested_same_axis_split_merges_with_scaled_flexes() {
    let nested = split(
        Axis::Horizontal,
        vec![
            group(0),
            split(Axis::Horizontal, vec![group(1), group(2)], vec![1.5, 0.5]),
        ],
        vec![1.0, 1.0],
    );

    assert_eq!(
        normalised(nested),
        split(
            Axis::Horizontal,
            vec![group(0), group(1), group(2)],
            vec![1.5, 1.125, 0.375]
        )
    );

    let collapsed = split(
        Axis::Horizontal,
        vec![
            group(0),
            split(
                Axis::Vertical,
                vec![split(
                    Axis::Horizontal,
                    vec![group(1), group(2)],
                    vec![0.5, 1.5],
                )],
                vec![1.0],
            ),
        ],
        vec![1.0, 1.0],
    );
    assert_eq!(
        normalised(collapsed),
        split(
            Axis::Horizontal,
            vec![group(0), group(1), group(2)],
            vec![1.5, 0.375, 1.125]
        )
    );
}

#[test]
fn invalid_flexes_reset_to_one() {
    for flexes in [
        vec![2.0],
        vec![1.0, 1.0, 1.0],
        vec![f32::NAN, 2.0],
        vec![f32::INFINITY, 1.0],
        vec![0.0, 2.0],
        vec![-1.0, 3.0],
        vec![1.0, 1.5],
        vec![1.0, 0.998],
    ] {
        assert_eq!(
            normalised(split(
                Axis::Vertical,
                vec![group(0), group(1)],
                flexes.clone()
            )),
            split(Axis::Vertical, vec![group(0), group(1)], vec![1.0, 1.0]),
            "{flexes:?}"
        );
    }

    let Member::Split { flexes, .. } = normalised(split(
        Axis::Vertical,
        vec![group(0), group(1)],
        vec![1.5, 0.5005],
    )) else {
        panic!("two children stay a split");
    };
    assert!((flexes[0] - 1.5 * 2.0 / 2.0005).abs() < 1e-6, "{flexes:?}");
    assert!(
        (flexes.iter().sum::<f32>() - 2.0).abs() < 1e-6,
        "{flexes:?}"
    );
}

#[test]
fn rects_tile_the_unit_square() {
    let (mut layout, [first, second, third]) = grid();
    assert_eq!(
        layout.rects(Surface::Center),
        [
            (first, rect(0.0, 0.0, 0.5, 1.0)),
            (second, rect(0.5, 0.0, 0.5, 0.5)),
            (third, rect(0.5, 0.5, 0.5, 0.5))
        ]
    );

    let fourth = layout
        .split(first, SplitDirection::Right, 1.0, no_min)
        .unwrap();
    assert_eq!(
        layout.rects(Surface::Center),
        [
            (first, rect(0.0, 0.0, 0.25, 1.0)),
            (fourth, rect(0.25, 0.0, 0.25, 1.0)),
            (second, rect(0.5, 0.0, 0.5, 0.5)),
            (third, rect(0.5, 0.5, 0.5, 0.5))
        ]
    );
}

#[test]
fn top_groups_skip_a_lower_group() {
    let (mut layout, [first, second, third]) = grid();
    assert_eq!(layout.top_groups(Surface::Center), [first, second]);

    let fourth = layout
        .split(first, SplitDirection::Down, 1.0, no_min)
        .unwrap();
    assert_eq!(layout.center_groups(), [first, fourth, second, third]);
    assert_eq!(layout.top_groups(Surface::Center), [first, second]);

    let fifth = layout
        .split(second, SplitDirection::Right, 1.0, no_min)
        .unwrap();
    assert_eq!(layout.top_groups(Surface::Center), [first, second, fifth]);

    let layout = Layout {
        center: split(Axis::Vertical, vec![group(0), group(3)], vec![0.002, 1.998]),
        ..Layout::default()
    };
    assert_eq!(
        layout.rects(Surface::Center)[1].1,
        rect(0.0, 0.001, 1.0, 0.999)
    );
    assert_eq!(
        layout.top_groups(Surface::Center),
        [GroupId(0)],
        "an epsilon down is below the top"
    );
}

#[test]
fn a_deep_vertical_split_keeps_one_top_group() {
    let mut layout = Layout::default();
    let top = layout.center_groups()[0];
    for _ in 0..12 {
        layout
            .split(top, SplitDirection::Down, 1.0, no_min)
            .unwrap();
    }
    assert_eq!(layout.top_groups(Surface::Center), [top]);

    let layout = Layout {
        center: split(
            Axis::Vertical,
            vec![group(0), group(1)],
            vec![0.0005, 1.9995],
        ),
        ..Layout::default()
    };
    assert_eq!(layout.top_groups(Surface::Center), [GroupId(0)]);
}

#[test]
fn remove_in_a_nested_split_hands_the_room_to_the_sibling() {
    let (mut layout, [first, second, third]) = grid();
    layout.activate(third);

    assert_eq!(layout.remove(third), Some(second));

    assert_eq!(
        layout.center(),
        &split(
            Axis::Horizontal,
            vec![Member::Group(first), Member::Group(second)],
            vec![1.0, 1.0]
        )
    );
    assert_eq!(layout.last_center(), second);

    let (mut layout, [first, second, third]) = grid();
    assert_eq!(layout.remove(first), Some(second));
    assert_eq!(
        layout.center(),
        &split(
            Axis::Vertical,
            vec![Member::Group(second), Member::Group(third)],
            vec![1.0, 1.0]
        )
    );
    assert_eq!(layout.last_center(), second);

    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout
        .split(first, SplitDirection::Right, 1.0, no_min)
        .unwrap();
    let third = layout
        .split(first, SplitDirection::Down, 1.0, no_min)
        .unwrap();
    layout.activate(second);
    assert_eq!(layout.remove(second), Some(third));
    assert_eq!(
        layout.center(),
        &split(
            Axis::Vertical,
            vec![Member::Group(first), Member::Group(third)],
            vec![1.0, 1.0]
        )
    );
    assert_eq!(layout.last_center(), third);
}

#[test]
fn restored_assigns_ids_depth_first_centre_then_docks() {
    let center = shape_split(
        Axis::Horizontal,
        vec![
            Shape::Group,
            shape_split(
                Axis::Vertical,
                vec![Shape::Group, Shape::Group],
                vec![1.0, 1.0],
            ),
        ],
        vec![1.0, 1.0],
    );

    let mut layout = Layout::restored(&center, [Some(&Shape::Group); 2], [false, true], 0);

    assert_eq!(
        layout.center(),
        &split(
            Axis::Horizontal,
            vec![
                group(0),
                split(Axis::Vertical, vec![group(1), group(2)], vec![1.0, 1.0])
            ],
            vec![1.0, 1.0]
        )
    );
    assert_eq!(layout.dock_groups(Side::Left), [GroupId(3)]);
    assert_eq!(layout.dock_groups(Side::Right), [GroupId(4)]);
    assert_eq!(
        layout.groups().map(GroupId::raw).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(layout.last_center(), GroupId(0));
    assert!(!layout.is_shown(Side::Left));
    assert!(layout.is_shown(Side::Right));
    assert_eq!(
        layout
            .split(GroupId(2), SplitDirection::Right, 1.0, no_min)
            .unwrap(),
        GroupId(5)
    );

    let mut layout = Layout::restored(&Shape::Group, [None, Some(&Shape::Group)], [true, false], 0);
    assert_eq!(layout.dock(Side::Left), None);
    assert_eq!(layout.dock_groups(Side::Right), [GroupId(1)]);
    assert!(layout.is_shown(Side::Left));
    assert!(!layout.is_shown(Side::Right));
    assert_eq!(layout.add_dock(Side::Left), GroupId(2));

    let layout = Layout::restored(&Shape::Group, [Some(&Shape::Group), None], [true, true], 0);
    assert_eq!(layout.dock_groups(Side::Left), [GroupId(1)]);
    assert_eq!(layout.dock(Side::Right), None);
    assert_eq!(
        Layout::restored(&Shape::Group, [Some(&Shape::Group); 2], [true, true], 0),
        Layout::default()
    );
}

#[test]
fn restored_resets_invalid_flexes() {
    for flexes in [vec![5.0, 1.0], vec![1.0], vec![f32::NAN, 1.0]] {
        let center = shape_split(Axis::Vertical, vec![Shape::Group, Shape::Group], flexes);
        let layout = Layout::restored(&center, [None, None], [true, true], 0);
        assert_eq!(
            layout.center(),
            &split(Axis::Vertical, vec![group(0), group(1)], vec![1.0, 1.0])
        );
    }
    let center = shape_split(
        Axis::Vertical,
        vec![Shape::Group, Shape::Group],
        vec![1.5, 0.5],
    );
    let layout = Layout::restored(&center, [None, None], [true, true], 0);
    assert_eq!(
        layout.center(),
        &split(Axis::Vertical, vec![group(0), group(1)], vec![1.5, 0.5])
    );
}

#[test]
fn restored_normalises_a_one_child_split() {
    let center = shape_split(Axis::Horizontal, vec![Shape::Group], vec![1.0]);
    let layout = Layout::restored(&center, [None, None], [true, true], 0);
    assert_eq!(layout.center(), &group(0));
    assert_eq!(layout.last_center(), GroupId(0));

    let center = shape_split(
        Axis::Horizontal,
        vec![
            shape_split(Axis::Vertical, vec![Shape::Group], vec![1.0]),
            shape_split(
                Axis::Horizontal,
                vec![Shape::Group, Shape::Group],
                vec![1.0, 1.0],
            ),
        ],
        vec![1.0, 1.0],
    );
    let layout = Layout::restored(&center, [None, None], [true, true], 0);
    assert_eq!(
        layout.center(),
        &split(
            Axis::Horizontal,
            vec![group(0), group(1), group(2)],
            vec![1.5, 0.75, 0.75]
        )
    );
}

/// Only `group` has a minimum, `min` along `axis`; any other query refuses.
fn only(group: GroupId, axis: Axis, min: f32) -> impl Fn(GroupId, Axis) -> f32 {
    move |held, along| {
        if (held, along) == (group, axis) {
            min
        } else {
            f32::INFINITY
        }
    }
}

#[test]
fn a_split_fits_while_each_half_keeps_the_minimum() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let second = layout
        .split(first, SplitDirection::Right, 1.0, no_min)
        .unwrap();
    let hair = 100.0_f32.next_up();
    let fits =
        |group, axis, room, min| layout.fits_split(group, axis, room, only(group, axis, min));

    // Half the centre's width, all its height.
    assert!(fits(first, Axis::Horizontal, 400.0, 100.0));
    assert!(!fits(first, Axis::Horizontal, 400.0, hair));
    assert!(fits(second, Axis::Vertical, 200.0, 100.0));
    assert!(!fits(second, Axis::Vertical, 200.0, hair));
    assert!(fits(GroupId(1), Axis::Horizontal, 200.0, 100.0));
    assert!(!fits(GroupId(1), Axis::Horizontal, 200.0, hair));
    assert!(!layout.fits_split(GroupId(7), Axis::Horizontal, 1.0, no_min));
}

#[test]
fn a_refused_split_hands_out_no_id() {
    let mut layout = Layout::default();
    let first = layout.last_center();
    let hair = 100.0_f32.next_up();

    let refused = layout.split(
        first,
        SplitDirection::Down,
        200.0,
        only(first, Axis::Vertical, hair),
    );

    assert_eq!(refused, None);
    assert_eq!(layout, Layout::default());
    let split = layout.split(
        first,
        SplitDirection::Down,
        200.0,
        only(first, Axis::Vertical, 100.0),
    );
    assert_eq!(split, Some(GroupId(3)));
}

/// Splits with one child and nested same-axis splits included; flexes valid, so rects are defined.
fn tree() -> impl Strategy<Value = Member> {
    let leaf = Just(group(0));
    leaf.prop_recursive(4, 24, 4, |inner| {
        (
            prop_oneof![Just(Axis::Horizontal), Just(Axis::Vertical)],
            proptest::collection::vec((inner, 0.1f32..10.0), 1..4),
        )
            .prop_map(|(axis, children)| {
                let (children, mut flexes): (Vec<_>, Vec<_>) = children.into_iter().unzip();
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a count far below 2^24 converts to f32 exactly"
                )]
                let scale = children.len() as f32 / flexes.iter().sum::<f32>();
                for flex in &mut flexes {
                    *flex *= scale;
                }
                split(axis, children, flexes)
            })
    })
    .prop_map(|mut tree| {
        renumber(&mut tree, &mut 0);
        tree
    })
}

fn renumber(member: &mut Member, next: &mut u32) {
    match member {
        Member::Group(group) => {
            *group = GroupId(*next);
            *next += 1;
        }
        Member::Split { children, .. } => {
            for child in children {
                renumber(child, next);
            }
        }
    }
}

fn is_normalised(member: &Member) -> bool {
    let Member::Split {
        axis,
        children,
        flexes,
    } = member
    else {
        return true;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count far below 2^24 converts to f32 exactly"
    )]
    let len = children.len() as f32;
    children.len() > 1
        && flexes.len() == children.len()
        && flexes.iter().all(|&flex| flex.is_finite() && flex > 0.0)
        && (flexes.iter().sum::<f32>() - len).abs() < 0.001
        && children.iter().all(|child| {
            is_normalised(child)
                && !matches!(child, Member::Split { axis: inner, .. } if inner == axis)
        })
}

/// `Split` and `Remove` pick from every surface's groups, `Activate` from the centre's.
#[derive(Debug, Clone, Copy)]
pub(super) enum TreeOp {
    Split(usize, SplitDirection),
    Remove(usize),
    Activate(usize),
    Add(Side),
}

fn direction() -> impl Strategy<Value = SplitDirection> {
    prop_oneof![
        Just(SplitDirection::Up),
        Just(SplitDirection::Down),
        Just(SplitDirection::Left),
        Just(SplitDirection::Right),
    ]
}

pub(super) fn tree_op() -> impl Strategy<Value = TreeOp> {
    prop_oneof![
        (any::<usize>(), direction()).prop_map(|(pick, direction)| TreeOp::Split(pick, direction)),
        any::<usize>().prop_map(TreeOp::Remove),
        any::<usize>().prop_map(TreeOp::Activate),
        prop_oneof![Just(Side::Left), Just(Side::Right)].prop_map(TreeOp::Add),
    ]
}

/// The group `pick` names, unless it is the centre's only group, which no remove takes.
fn removable(layout: &Layout, pick: usize) -> Option<GroupId> {
    let groups: Vec<_> = layout.groups().collect();
    let group = groups[pick % groups.len()];
    (layout.surface_of(group) != Surface::Center || layout.center_groups().len() > 1)
        .then_some(group)
}

/// `ops` run on the default layout, as `tree_ops_keep_the_invariants` runs them.
pub(super) fn built(ops: &[TreeOp]) -> Layout {
    let mut layout = Layout::default();
    for &op in ops {
        let groups: Vec<_> = layout.groups().collect();
        let center = layout.center_groups();
        match op {
            TreeOp::Split(pick, direction) => {
                layout
                    .split(groups[pick % groups.len()], direction, 1.0, no_min)
                    .unwrap();
            }
            TreeOp::Remove(pick) => {
                if let Some(group) = removable(&layout, pick) {
                    layout.remove(group);
                }
            }
            TreeOp::Activate(pick) => layout.activate(center[pick % center.len()]),
            TreeOp::Add(side) => {
                if layout.dock(side).is_none() {
                    layout.add_dock(side);
                }
            }
        }
    }
    layout
}

const SURFACES: [Surface; 3] = [
    Surface::Center,
    Surface::Dock(Side::Left),
    Surface::Dock(Side::Right),
];

fn tree_of(layout: &Layout, surface: Surface) -> Option<&Member> {
    match surface {
        Surface::Center => Some(layout.center()),
        Surface::Dock(side) => layout.dock(side),
    }
}

proptest! {
    #[test]
    fn normalising_never_moves_a_rect(tree in tree()) {
        let before = rects(&tree);
        let after = rects(&normalised(tree));
        prop_assert_eq!(before.len(), after.len());
        for ((group, was), (now_group, now)) in before.into_iter().zip(after) {
            prop_assert_eq!(group, now_group);
            for (was, now) in [(was.x, now.x), (was.y, now.y), (was.width, now.width), (was.height, now.height)] {
                prop_assert!((was - now).abs() < 1e-4, "{group:?} moved from {was:?} to {now:?}");
            }
        }
    }

    #[test]
    fn tree_ops_keep_the_invariants(ops in proptest::collection::vec(tree_op(), 0..48)) {
        let mut layout = Layout::default();
        let mut handed: HashSet<GroupId> = layout.groups().collect();
        for op in ops {
            let groups: Vec<_> = layout.groups().collect();
            let center = layout.center_groups();
            match op {
                TreeOp::Split(pick, direction) => {
                    let group = groups[pick % groups.len()];
                    let surface = layout.surface_of(group);
                    let split = layout.split(group, direction, 1.0, no_min).unwrap();
                    prop_assert!(handed.insert(split), "{split:?} handed out twice");
                    prop_assert_eq!(layout.surface_of(split), surface);
                }
                TreeOp::Remove(pick) => {
                    let Some(group) = removable(&layout, pick) else { continue };
                    let surface = layout.surface_of(group);
                    let heir = layout.remove(group);
                    prop_assert!(!layout.groups().any(|held| held == group));
                    let held: Vec<_> = layout.rects(surface).into_iter().map(|(held, _)| held).collect();
                    if let Some(heir) = heir {
                        prop_assert!(held.contains(&heir));
                    } else {
                        prop_assert_ne!(surface, Surface::Center);
                        prop_assert!(held.is_empty());
                    }
                }
                TreeOp::Activate(pick) => {
                    let group = center[pick % center.len()];
                    layout.activate(group);
                    prop_assert_eq!(layout.last_center(), group);
                }
                TreeOp::Add(side) => {
                    if layout.dock(side).is_none() {
                        let added = layout.add_dock(side);
                        prop_assert!(handed.insert(added), "{added:?} handed out twice");
                    }
                }
            }
            let mut tiled = Vec::new();
            for surface in SURFACES {
                let rects = layout.rects(surface);
                if let Some(member) = tree_of(&layout, surface) {
                    prop_assert!(is_normalised(member), "{surface:?}: {member:?}");
                    let area: f32 = rects.iter().map(|(_, rect)| rect.width * rect.height).sum();
                    prop_assert!((area - 1.0).abs() < 1e-3, "{surface:?}: {area}");
                } else {
                    prop_assert!(rects.is_empty());
                }
                tiled.extend(rects.into_iter().map(|(group, _)| group));
            }
            let groups: Vec<_> = layout.groups().collect();
            prop_assert_eq!(&groups, &tiled);
            let unique: HashSet<_> = groups.iter().collect();
            prop_assert_eq!(unique.len(), groups.len());
            prop_assert!(layout.center_groups().contains(&layout.last_center()));
        }
    }
    #[test]
    fn a_split_is_refused_exactly_where_it_does_not_fit(
        ops in proptest::collection::vec(tree_op(), 0..24),
        pick in any::<usize>(),
        direction in direction(),
        room in 0.0f32..2000.0,
        min in 0.0f32..500.0,
    ) {
        let mut layout = built(&ops);
        let groups: Vec<_> = layout.groups().collect();
        let group = groups[pick % groups.len()];
        let before = layout.clone();
        let fits = layout.fits_split(group, direction.axis(), room, |_, _| min);

        let split = layout.split(group, direction, room, |_, _| min);

        prop_assert_eq!(split.is_some(), fits);
        if let Some(fresh) = split {
            prop_assert_eq!(layout.surface_of(fresh), before.surface_of(group));
        } else {
            prop_assert_eq!(layout, before);
        }
    }

    #[test]
    fn restored_keeps_a_built_layouts_rects(
        ops in proptest::collection::vec(tree_op(), 0..48),
        shown in any::<[bool; 2]>(),
        active in 0..8usize,
    ) {
        let built = built(&ops);
        let docks = [Side::Left, Side::Right].map(|side| built.dock(side).map(shape));

        let restored = Layout::restored(&shape(built.center()), docks.each_ref().map(Option::as_ref), shown, active);

        for surface in SURFACES {
            let (was, now) = (built.rects(surface), restored.rects(surface));
            prop_assert_eq!(was.len(), now.len());
            for ((_, was), (_, now)) in was.into_iter().zip(now) {
                for (was, now) in [(was.x, now.x), (was.y, now.y), (was.width, now.width), (was.height, now.height)] {
                    prop_assert!((was - now).abs() < 1e-4, "{surface:?}: {was:?} became {now:?}");
                }
            }
            if let Some(member) = tree_of(&restored, surface) {
                prop_assert!(is_normalised(member), "{surface:?}: {member:?}");
            }
        }
        let count = built.groups().count();
        let ids: Vec<_> = restored.groups().map(GroupId::raw).collect();
        prop_assert_eq!(ids, (0..count).map(|ix| u32::try_from(ix).unwrap()).collect::<Vec<_>>());
        let center = restored.center_groups();
        prop_assert_eq!(restored.last_center(), *center.get(active).unwrap_or(&center[0]));
        prop_assert_eq!([restored.is_shown(Side::Left), restored.is_shown(Side::Right)], shown);
    }
}
