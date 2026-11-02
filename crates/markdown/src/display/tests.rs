use std::ops::Range;

use inkra_text::{Bias, ByteOffset};
use proptest::prelude::*;

use super::{DisplayLine, DisplayOffset};

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

fn hide(start: usize, end: usize) -> (Range<ByteOffset>, &'static str) {
    (at(start)..at(end), "")
}

#[test]
fn a_line_with_nothing_hidden_is_the_identity() {
    let line = DisplayLine::new(at(10)..at(13), []);
    assert!(line.is_identity());
    assert_eq!(line.text("abc"), "abc");
    assert_eq!(line.to_display(at(12)), DisplayOffset(2));
    assert_eq!(line.to_buffer(DisplayOffset(2), Bias::Left), at(12));
}

#[test]
fn hidden_delimiters_leave_the_content() {
    // "a **b** c" at offset 0: hide 2..4 and 5..7.
    let line = DisplayLine::new(at(0)..at(9), [hide(2, 4), hide(5, 7)]);
    assert_eq!(line.text("a **b** c"), "a b c");
    assert_eq!(line.to_display(at(2)), DisplayOffset(2));
    assert_eq!(line.to_display(at(4)), DisplayOffset(2));
    assert_eq!(line.to_display(at(5)), DisplayOffset(3));
    assert_eq!(line.to_display(at(7)), DisplayOffset(3));
    assert_eq!(line.to_display(at(9)), DisplayOffset(5));
}

#[test]
fn hidden_lists_the_ranges_in_order() {
    let line = DisplayLine::new(at(0)..at(9), [hide(2, 4), hide(5, 7)]);
    let hidden: Vec<_> = line.hidden().cloned().collect();
    assert_eq!(hidden, vec![at(2)..at(4), at(5)..at(7)]);
}

#[test]
fn an_offset_inside_a_hidden_range_clamps_to_its_start() {
    let line = DisplayLine::new(at(0)..at(9), [hide(2, 4)]);
    assert_eq!(line.to_display(at(3)), DisplayOffset(2));
}

#[test]
fn bias_picks_either_side_of_a_gap() {
    let line = DisplayLine::new(at(0)..at(9), [hide(2, 4), hide(5, 7)]);
    assert_eq!(line.to_buffer(DisplayOffset(2), Bias::Left), at(2));
    assert_eq!(line.to_buffer(DisplayOffset(2), Bias::Right), at(4));
    assert_eq!(line.to_buffer(DisplayOffset(3), Bias::Left), at(5));
    assert_eq!(line.to_buffer(DisplayOffset(3), Bias::Right), at(7));
}

#[test]
fn abutting_hidden_ranges_are_one_gap() {
    // "**a***b*": hide 0..2, 3..5, 5..6, 7..8 -> "ab".
    let line = DisplayLine::new(
        at(0)..at(8),
        [hide(0, 2), hide(3, 5), hide(5, 6), hide(7, 8)],
    );
    assert_eq!(line.text("**a***b*"), "ab");
    assert_eq!(line.to_buffer(DisplayOffset(1), Bias::Left), at(3));
    assert_eq!(line.to_buffer(DisplayOffset(1), Bias::Right), at(6));
}

#[test]
fn a_replacement_maps_its_ends_and_clamps_inside() {
    // "- a": replace 0..1 with "•" (3 bytes) -> "• a".
    let line = DisplayLine::new(at(0)..at(3), [(at(0)..at(1), "•")]);
    assert_eq!(line.text("- a"), "• a");
    assert_eq!(line.to_display(at(0)), DisplayOffset(0));
    assert_eq!(line.to_display(at(1)), DisplayOffset(3));
    assert_eq!(line.to_display(at(2)), DisplayOffset(4));
    assert_eq!(line.to_buffer(DisplayOffset(3), Bias::Left), at(1));
    assert_eq!(line.to_buffer(DisplayOffset(1), Bias::Left), at(0));
    assert_eq!(line.to_buffer(DisplayOffset(1), Bias::Right), at(1));
}

#[test]
fn past_the_end_clamps_to_the_line_end() {
    let line = DisplayLine::new(at(0)..at(5), [hide(3, 5)]);
    assert_eq!(line.to_buffer(DisplayOffset(99), Bias::Left), at(5));
    assert_eq!(line.to_display(at(99)), DisplayOffset(3));
}

/// A line of text with hidden ranges on char boundaries, sorted, non-overlapping.
fn line_and_hidden() -> impl Strategy<Value = (String, Vec<(usize, usize, bool)>)> {
    "[a-zé*•]{0,24}"
        .prop_flat_map(|text| {
            let bounds: Vec<usize> = (0..=text.len())
                .filter(|&i| text.is_char_boundary(i))
                .collect();
            let picks =
                prop::collection::vec((0..bounds.len(), 0..bounds.len(), any::<bool>()), 0..4);
            (Just(text), Just(bounds), picks)
        })
        .prop_map(|(text, bounds, picks)| {
            let mut ranges: Vec<(usize, usize, bool)> = picks
                .into_iter()
                .map(|(a, b, bullet)| {
                    let (a, b) = (bounds[a.min(b)], bounds[a.max(b)]);
                    (a, b, bullet)
                })
                .filter(|(a, b, _)| a < b)
                .collect();
            ranges.sort_unstable();
            let mut kept: Vec<(usize, usize, bool)> = Vec::new();
            for range in ranges {
                if kept.last().is_none_or(|last| last.1 <= range.0) {
                    kept.push(range);
                }
            }
            (text, kept)
        })
}

proptest! {
    #[test]
    fn the_maps_are_monotonic_and_round_trip((text, hidden) in line_and_hidden(), start in 0usize..50) {
        let line = DisplayLine::new(
            at(start)..at(start + text.len()),
            hidden.iter().map(|&(a, b, bullet)| {
                (at(start + a)..at(start + b), if bullet { "•" } else { "" })
            }),
        );
        let shown = line.text(&text);
        let inside = |o: usize| hidden.iter().any(|&(a, b, _)| a < o && o < b);

        let mut previous = DisplayOffset(0);
        for o in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
            let d = line.to_display(at(start + o));
            prop_assert!(d >= previous, "to_display not monotonic at {o}");
            prop_assert!(d.0 <= shown.len());
            previous = d;
            if !inside(o) {
                prop_assert!(line.to_buffer(d, Bias::Left) <= at(start + o));
                prop_assert!(line.to_buffer(d, Bias::Right) >= at(start + o));
            }
        }
        for d in (0..=shown.len()).filter(|&d| shown.is_char_boundary(d)) {
            for bias in [Bias::Left, Bias::Right] {
                let o = line.to_buffer(DisplayOffset(d), bias);
                prop_assert!(!inside(o.get() - start), "to_buffer landed inside a hidden range");
                prop_assert_eq!(line.to_display(o), DisplayOffset(d));
            }
        }
    }
}
