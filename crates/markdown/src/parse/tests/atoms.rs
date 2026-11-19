//! `Markdown::snap`: an item part with no mark that holds blanks is an atom (task boxes spec, D2).

use std::ops::Range;

use inkra_text::{Bias, ByteOffset};

use super::super::parse;

/// Each offset of `text` snapped with `bias`.
fn snapped(text: &str, bias: Bias) -> Vec<usize> {
    let markdown = parse(text);
    (0..=text.len())
        .map(|offset| markdown.snap(ByteOffset::new(offset), bias).get())
        .collect()
}

/// `text`'s offsets, each one inside an atom moved to the edge `bias` picks. An atom is its inside,
/// its start edge and its text start.
fn expected(text: &str, atoms: &[(Range<usize>, usize, usize)], bias: Bias) -> Vec<usize> {
    (0..=text.len())
        .map(|offset| {
            atoms
                .iter()
                .find(|(inside, _, _)| inside.contains(&offset))
                .map_or(offset, |&(_, edge, text_start)| match bias {
                    Bias::Left => edge,
                    Bias::Right => text_start,
                })
        })
        .collect()
}

#[test]
fn an_atom_snaps_left_to_the_previous_lines_end_and_right_to_the_text_start() {
    for (text, atoms) in [
        ("- a\n  b", &[(4..6, 3, 6)][..]),
        ("- a\n\n  b", &[(5..7, 4, 7)]),
        ("> - a\n>   b", &[(6..10, 5, 10)]),
        (
            "- [ ] a\n\n  ```\n  def f():\n      return 1\n  ```",
            &[
                (9..11, 8, 11),
                (15..17, 14, 17),
                (26..28, 25, 28),
                (41..43, 40, 43),
            ],
        ),
        ("- a\n  ```\n\tx\n  ```", &[(4..6, 3, 6), (13..15, 12, 15)]),
    ] {
        for bias in [Bias::Left, Bias::Right] {
            assert_eq!(
                snapped(text, bias),
                expected(text, atoms, bias),
                "{text:?} {bias:?}"
            );
        }
    }
}

#[test]
fn a_marker_line_an_empty_item_part_and_a_bare_quote_are_no_atoms() {
    for text in ["- a", "- [ ] a\n- b", "- a\nb", "> - a\n> b", "> a\n> b"] {
        for bias in [Bias::Left, Bias::Right] {
            assert_eq!(snapped(text, bias), expected(text, &[], bias), "{text:?}");
        }
    }
}
