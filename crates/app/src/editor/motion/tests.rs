use proptest::prelude::*;

use super::*;

const BYTE: f32 = 10.;

/// Rows as `(start, caret_end)`, 10 px per byte. A row sharing its start with the next
/// holds no caret: that offset belongs to the next row, as in `DocumentLayout`.
#[derive(Debug)]
struct Grid(Vec<(usize, usize)>);

impl Grid {
    /// Rows `widths` bytes wide, each ending in a newline.
    fn lines(widths: &[usize]) -> Self {
        let mut start = 0;
        Self(
            widths
                .iter()
                .map(|&width| {
                    let row = (start, start + width);
                    start += width + 1;
                    row
                })
                .collect(),
        )
    }
}

impl CaretRows for Grid {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn row_for_offset(&self, offset: ByteOffset) -> usize {
        self.0
            .iter()
            .rposition(|&(start, _)| start <= offset.get())
            .unwrap_or(0)
    }

    fn holds_caret(&self, row: usize) -> bool {
        self.row_for_offset(ByteOffset::new(self.0[row].0)) == row
    }

    fn x_for_offset(&self, offset: ByteOffset) -> Pixels {
        let (start, _) = self.0[self.row_for_offset(offset)];
        #[allow(clippy::cast_precision_loss)]
        px((offset.get() - start) as f32 * BYTE)
    }

    fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset {
        let (start, caret_end) = self.0[row];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bytes = (f32::from(x) / BYTE).round().max(0.) as usize;
        ByteOffset::new((start + bytes).min(caret_end))
    }
}

#[test]
fn down_keeps_the_goal_across_a_short_row() {
    let grid = Grid::lines(&[30, 5, 30]);
    let goal = Goal::Column(200.);

    let (short, goal) = vertical(&grid, ByteOffset::new(20), goal, Vertical::Down).unwrap();
    assert_eq!(short, ByteOffset::new(31 + 5));
    let (landing, goal) = vertical(&grid, short, goal, Vertical::Down).unwrap();

    assert_eq!(landing, ByteOffset::new(37 + 20));
    assert_eq!(goal, Goal::Column(200.));
}

#[test]
fn a_move_without_a_goal_starts_from_the_caret_x() {
    let grid = Grid::lines(&[30, 30]);

    let moved = vertical(&grid, ByteOffset::new(31 + 7), Goal::None, Vertical::Up);

    assert_eq!(moved, Some((ByteOffset::new(7), Goal::Column(70.))));
}

#[test]
fn down_steps_over_a_row_that_holds_no_caret() {
    let grid = Grid(vec![(0, 10), (11, 11), (11, 20)]);

    let moved = vertical(&grid, ByteOffset::new(4), Goal::None, Vertical::Down);

    assert_eq!(moved, Some((ByteOffset::new(15), Goal::Column(40.))));
}

#[test]
fn up_from_the_first_row_is_none() {
    let grid = Grid::lines(&[30, 30]);
    assert_eq!(
        vertical(&grid, ByteOffset::new(4), Goal::None, Vertical::Up),
        None
    );
}

#[test]
fn down_from_the_last_row_is_none() {
    let grid = Grid::lines(&[30, 30]);
    assert_eq!(
        vertical(&grid, ByteOffset::new(35), Goal::None, Vertical::Down),
        None
    );
}

/// Rows of up to 8 bytes; a collapsed row shares its start with the next.
fn grids() -> impl Strategy<Value = Grid> {
    prop::collection::vec((0usize..8, any::<bool>()), 1..12).prop_map(|rows| {
        let last = rows.len() - 1;
        let mut start = 0;
        Grid(
            rows.into_iter()
                .enumerate()
                .map(|(index, (width, collapsed))| {
                    if collapsed && index < last {
                        (start, start)
                    } else {
                        let row = (start, start + width);
                        start += width + 1;
                        row
                    }
                })
                .collect(),
        )
    })
}

proptest! {
    #[test]
    fn a_vertical_move_lands_on_a_caret_row_in_its_direction(
        grid in grids(),
        cursor in any::<prop::sample::Index>(),
        goal in prop::option::of(0f32..100.),
        down in any::<bool>(),
    ) {
        let end = grid.0.last().unwrap().1;
        let cursor = ByteOffset::new(cursor.index(end + 1));
        let goal = goal.map_or(Goal::None, Goal::Column);
        let direction = if down { Vertical::Down } else { Vertical::Up };
        let from = grid.row_for_offset(cursor);
        let ahead = |row: usize| if down { row > from } else { row < from };
        let reachable = (0..grid.row_count()).any(|row| ahead(row) && grid.holds_caret(row));

        let moved = vertical(&grid, cursor, goal, direction);

        prop_assert_eq!(moved.is_some(), reachable);
        if let Some((landing, _)) = moved {
            let row = grid.row_for_offset(landing);
            prop_assert!(grid.holds_caret(row), "landed on row {row}, which holds no caret");
            prop_assert!(ahead(row), "landed on row {row}, not past row {from}");
        }
    }
}
