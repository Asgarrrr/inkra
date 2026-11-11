use std::ops::Range;

use gpui::Hsla;
use proptest::prelude::*;

use super::render::highlights;
use super::{LitText, PathText};
use crate::one_line::one_line;
use crate::theme::builtin;
use crate::ui::Color;

/// Pieces that put control chars, a `/`, multi-byte chars and a combining mark into a text.
fn text() -> impl Strategy<Value = String> {
    const PIECES: &[&str] = &["a", "b", " ", "\t", "\n", "/", "é", "e\u{301}", "🎉"];
    prop::collection::vec(prop::sample::select(PIECES), 0..12).prop_map(|p| p.concat())
}

/// A text and any ranges, ends up to 8 bytes past it, reversed ones too.
fn text_and_ranges() -> impl Strategy<Value = (String, Vec<Range<usize>>)> {
    text().prop_flat_map(|text| {
        let end = text.len() + 8;
        let ranges = prop::collection::vec((0..end, 0..end).prop_map(|(a, b)| a..b), 0..6);
        (Just(text), ranges)
    })
}

/// A text and ranges on its char boundaries, ascending, non-empty and not touching.
fn valid() -> impl Strategy<Value = (String, Vec<Range<usize>>)> {
    text().prop_flat_map(|text| {
        let bounds: Vec<usize> = text
            .char_indices()
            .map(|(ix, _)| ix)
            .chain([text.len()])
            .collect();
        let count = bounds.len();
        let ranges = prop::sample::subsequence(bounds, 0..=count).prop_map(|picked| {
            picked
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[start, end]| start..end)
                .collect()
        });
        (Just(text), ranges)
    })
}

fn holds_invariants(lit: &LitText) -> Result<(), TestCaseError> {
    let text = lit.text();
    prop_assert!(
        !text.contains(|c: char| c.is_ascii_control()),
        "I1: {text:?}"
    );
    for range in lit.ranges() {
        prop_assert!(range.start < range.end, "I2: {range:?} is empty");
        prop_assert!(range.end <= text.len(), "I2: {range:?} past {text:?}");
        prop_assert!(
            text.is_char_boundary(range.start),
            "I2: {range:?} in {text:?}"
        );
        prop_assert!(
            text.is_char_boundary(range.end),
            "I2: {range:?} in {text:?}"
        );
    }
    for pair in lit.ranges().windows(2) {
        prop_assert!(pair[0].end < pair[1].start, "I3: {pair:?}");
    }
    Ok(())
}

fn lit_in(ranges: &[Range<usize>], byte: usize) -> bool {
    ranges.iter().any(|range| range.contains(&byte))
}

proptest! {
    #[test]
    fn any_input_gives_valid_ranges((text, ranges) in text_and_ranges()) {
        holds_invariants(&LitText::line(&text, ranges.clone()))?;
        holds_invariants(&LitText::context(&text, ranges.clone()))?;
        let path = PathText::new(&text, &ranges);
        holds_invariants(&path.name)?;
        holds_invariants(&path.folder)?;
    }

    #[test]
    fn a_split_keeps_the_text_and_starts_the_rest_at_its_first_range(
        (text, ranges) in text_and_ranges()
    ) {
        let lit = LitText::line(&text, ranges);
        let (before, rest) = lit.split_at_first();
        holds_invariants(&rest)?;
        prop_assert_eq!(format!("{before}{}", rest.text()), lit.text().to_string());
        prop_assert_eq!(rest.ranges().len(), lit.ranges().len());
        for (range, shifted) in lit.ranges().iter().zip(rest.ranges()) {
            prop_assert_eq!(&lit.text()[range.clone()], &rest.text()[shifted.clone()]);
        }
        prop_assert!(rest.ranges().first().is_none_or(|first| first.start == 0));
    }

    #[test]
    fn valid_input_keeps_its_ranges((text, ranges) in valid()) {
        let line = LitText::line(&text, ranges.clone());
        prop_assert_eq!(line.text().len(), text.len());
        prop_assert_eq!(line.ranges(), &ranges[..]);

        let context = LitText::context(&text, ranges.clone());
        let lit = |lit: &LitText| {
            lit.ranges().iter().map(|range| lit.text()[range.clone()].to_owned()).collect::<Vec<_>>()
        };
        let expected: Vec<String> =
            ranges.iter().map(|range| one_line(&text[range.clone()]).into_owned()).collect();
        prop_assert_eq!(lit(&context), expected);
    }

    #[test]
    fn a_path_lights_each_byte_in_its_half((path, ranges) in valid()) {
        let split = PathText::new(&path, &ranges);
        let stem_start = path.rfind('/').map_or(0, |slash| slash + 1);
        let folder_end = stem_start.saturating_sub(1);
        prop_assert_eq!(split.folder.text().len(), folder_end);
        prop_assert_eq!(split.name.text().len(), path.len() - stem_start);
        for byte in 0..folder_end {
            prop_assert_eq!(lit_in(split.folder.ranges(), byte), lit_in(&ranges, byte), "{}", byte);
        }
        for byte in stem_start..path.len() {
            let lit = lit_in(split.name.ranges(), byte - stem_start);
            prop_assert_eq!(lit, lit_in(&ranges, byte), "{}", byte);
        }
    }
}

#[test]
fn a_range_ending_on_the_last_slash_lights_the_folder_only() {
    let path = PathText::new("ab/plan", &[Range { start: 0, end: 3 }]);

    assert_eq!(path.folder.ranges(), [Range { start: 0, end: 2 }]);
    assert!(path.name.ranges().is_empty());
}

#[test]
fn a_range_across_the_last_slash_lights_both_halves() {
    let path = PathText::new("ab/plan", &[Range { start: 1, end: 5 }]);

    assert_eq!(path.folder.ranges(), [Range { start: 1, end: 2 }]);
    assert_eq!(path.name.ranges(), [Range { start: 0, end: 2 }]);
}

#[test]
fn a_root_path_has_no_folder() {
    let path = PathText::new("plan", &[Range { start: 0, end: 2 }]);

    assert_eq!(path.folder, LitText::default());
    assert_eq!(path.name.text().as_ref(), "plan");
    assert_eq!(path.name.ranges(), [Range { start: 0, end: 2 }]);
}

#[test]
fn label_joins_folder_and_name() {
    assert_eq!(PathText::new("work/a\nb", &[]).label().as_ref(), "work/a b");
    assert_eq!(PathText::new("plan", &[]).label().as_ref(), "plan");
}

#[test]
fn a_mid_char_range_widens_to_the_whole_char() {
    // `é` is bytes 1..3.
    let lit = LitText::line("aé b", [Range { start: 2, end: 3 }]);

    assert_eq!(lit.ranges(), [Range { start: 1, end: 3 }]);
}

#[test]
fn overlapping_ranges_merge() {
    let lit = LitText::line("abcdef", [4..5, 0..2, 1..3, 3..4]);

    assert_eq!(lit.ranges(), [Range { start: 0, end: 5 }]);
    assert_eq!(LitText::line("abcdef", [4..5, 0..2]).ranges(), [0..2, 4..5]);
}

#[test]
fn a_range_past_the_end_is_clamped() {
    let lit = LitText::line("abc", [1..10, 7..9]);

    assert_eq!(lit.ranges(), [Range { start: 1, end: 3 }]);
}

#[test]
fn the_indent_is_cut_but_never_past_the_hit() {
    let hit = LitText::context("\t  we plan", [Range { start: 6, end: 10 }]);
    assert_eq!(hit.text().as_ref(), "we plan");
    assert_eq!(hit.ranges(), [Range { start: 3, end: 7 }]);

    let in_indent = LitText::context("   x", [Range { start: 1, end: 4 }]);
    assert_eq!(in_indent.text().as_ref(), "  x");
    assert_eq!(in_indent.ranges(), [Range { start: 0, end: 3 }]);

    assert_eq!(LitText::context("\t a", []).text().as_ref(), "a");
}

#[test]
fn highlights_colour_every_range() {
    let lit = LitText::line("ab cd", [0..1, 3..4]);
    let color = Hsla::from(Color::Highlight.rgba(builtin::dark()));

    let lit: Vec<_> = highlights(&lit, color).collect();

    assert_eq!(
        lit.iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>(),
        [0..1, 3..4]
    );
    assert!(lit.iter().all(|(_, style)| style.color == Some(color)));
}
