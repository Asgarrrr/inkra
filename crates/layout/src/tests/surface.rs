use crate::{
    Axis, GroupId, Layout, Member, Rect, Shape, Side, SplitDirection, StoredDock, Surface, View,
    restore_views,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn stored(names: &[&str], active: usize) -> StoredDock {
    StoredDock {
        views: names.iter().map(|&name| name.to_owned()).collect(),
        active,
    }
}

#[test]
fn surface_of_names_each_surface() {
    let mut layout = Layout::default();
    assert_eq!(layout.surface_of(GroupId(0)), Surface::Center);
    assert_eq!(layout.surface_of(GroupId(1)), Surface::Dock(Side::Left));
    assert_eq!(layout.surface_of(GroupId(2)), Surface::Dock(Side::Right));

    let lower = layout.split(GroupId(1), SplitDirection::Down);

    assert_eq!(layout.surface_of(lower), Surface::Dock(Side::Left));
    assert_eq!(layout.dock_groups(Side::Left), [GroupId(1), lower]);
    assert_eq!(layout.dock_groups(Side::Right), [GroupId(2)]);
    assert_eq!(
        layout.rects(Surface::Dock(Side::Left)),
        [
            (GroupId(1), rect(0.0, 0.0, 1.0, 0.5)),
            (lower, rect(0.0, 0.5, 1.0, 0.5))
        ]
    );
    assert_eq!(layout.top_groups(Surface::Dock(Side::Left)), [GroupId(1)]);
    assert_eq!(layout.top_groups(Surface::Dock(Side::Right)), [GroupId(2)]);
    assert_eq!(layout.center_groups(), [GroupId(0)]);
    assert_eq!(layout.last_center(), GroupId(0));
    assert_eq!(
        layout.groups().collect::<Vec<_>>(),
        [GroupId(0), GroupId(1), lower, GroupId(2)]
    );
}

#[test]
#[should_panic(expected = "is not in the layout")]
fn surface_of_an_absent_group_panics() {
    let _ = Layout::default().surface_of(GroupId(7));
}

#[test]
fn remove_a_dock_group_hands_its_room_to_its_sibling() {
    let mut layout = Layout::default();
    let top = GroupId(2);
    let middle = layout.split(top, SplitDirection::Down);
    let bottom = layout.split(middle, SplitDirection::Down);

    assert_eq!(layout.remove(middle), Some(top));

    assert_eq!(
        layout.dock(Side::Right),
        Some(&Member::Split {
            axis: Axis::Vertical,
            children: vec![Member::Group(top), Member::Group(bottom)],
            flexes: vec![1.5, 0.5],
        })
    );
    assert_eq!(layout.remove(top), Some(bottom));
    assert_eq!(layout.dock(Side::Right), Some(&Member::Group(bottom)));
    assert_eq!(layout.center_groups(), [GroupId(0)]);
    assert_eq!(layout.last_center(), GroupId(0));
}

#[test]
fn removing_a_docks_last_group_empties_it_and_keeps_shown() {
    let mut layout = Layout::default();
    layout.toggle(Side::Left);
    let lower = layout.split(GroupId(1), SplitDirection::Right);
    assert_eq!(layout.remove(GroupId(1)), Some(lower));

    assert_eq!(layout.remove(lower), None);

    assert_eq!(layout.dock(Side::Left), None);
    assert!(layout.dock_groups(Side::Left).is_empty());
    assert!(layout.rects(Surface::Dock(Side::Left)).is_empty());
    assert!(layout.top_groups(Surface::Dock(Side::Left)).is_empty());
    assert!(!layout.is_shown(Side::Left));
    layout.toggle(Side::Left);
    assert!(layout.is_shown(Side::Left));
    assert!(!layout.is_drawn(Side::Left));
    assert_eq!(layout.add_dock(Side::Left), GroupId(4));
    assert!(layout.is_drawn(Side::Left));
}

#[test]
fn restored_honours_the_saved_active() {
    let row = Shape::Split {
        axis: Axis::Horizontal,
        children: vec![Shape::Group, Shape::Group, Shape::Group],
        flexes: vec![1.0, 1.0, 1.0],
    };
    let column = Shape::Split {
        axis: Axis::Vertical,
        children: vec![
            Shape::Group,
            Shape::Split {
                axis: Axis::Vertical,
                children: vec![Shape::Group],
                flexes: vec![1.0],
            },
        ],
        flexes: vec![1.0, 1.0],
    };

    let layout = Layout::restored(&row, [Some(&column), None], [true, false], 2);

    assert_eq!(layout.last_center(), GroupId(2));
    assert_eq!(
        layout.dock(Side::Left),
        Some(&Member::Split {
            axis: Axis::Vertical,
            children: vec![Member::Group(GroupId(3)), Member::Group(GroupId(4))],
            flexes: vec![1.0, 1.0],
        })
    );
    assert_eq!(layout.dock(Side::Right), None);
    assert!(layout.is_shown(Side::Left));
    assert!(!layout.is_shown(Side::Right));
    assert_eq!(
        layout.groups().map(GroupId::raw).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );

    let right = Layout::restored(&row, [None, Some(&column)], [true, true], 0);
    assert_eq!(right.dock_groups(Side::Right), [GroupId(3), GroupId(4)]);
    assert_eq!(right.last_center(), GroupId(0));
}

#[test]
fn an_active_out_of_range_falls_back_to_the_first_centre_group() {
    let row = Shape::Split {
        axis: Axis::Horizontal,
        children: vec![Shape::Group, Shape::Group],
        flexes: vec![1.0, 1.0],
    };
    for active in [2, 3, usize::MAX] {
        let layout = Layout::restored(&row, [None, None], [true, true], active);
        assert_eq!(layout.last_center(), GroupId(0), "{active}");
    }
    let layout = Layout::restored(&row, [None, None], [true, true], 1);
    assert_eq!(layout.last_center(), GroupId(1));
    assert_eq!(
        Layout::restored(&Shape::Group, [Some(&Shape::Group); 2], [true, true], 0),
        Layout::default()
    );
}

#[test]
fn restore_views_per_group_rules() {
    let views = restore_views([
        &[
            stored(&["graph", "files"], 1),
            stored(&["files", "backlinks"], 1),
        ],
        &[stored(&["backlinks"], 0)],
    ]);
    assert_eq!(
        views,
        [
            vec![(vec![View::Files], 0), (vec![View::Backlinks], 0)],
            vec![(Vec::new(), 0)],
        ]
    );

    let views = restore_views([&[stored(&[], 0), stored(&["backlinks"], 0)], &[]]);
    assert_eq!(
        views,
        [
            vec![(vec![View::Files], 0), (vec![View::Backlinks], 0)],
            Vec::new()
        ]
    );

    let views = restore_views([&[], &[]]);
    assert_eq!(
        views,
        [
            vec![(vec![View::Files], 0)],
            vec![(vec![View::Backlinks], 0)]
        ]
    );

    let views = restore_views([
        &[stored(&["backlinks", "graph", "files"], 2)],
        &[stored(&[], 0), stored(&["graph"], 4)],
    ]);
    assert_eq!(
        views,
        [
            vec![(vec![View::Backlinks, View::Files], 1)],
            vec![(Vec::new(), 0), (Vec::new(), 0)],
        ]
    );
}
