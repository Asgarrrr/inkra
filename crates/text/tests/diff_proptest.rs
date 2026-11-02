use std::ops::Range;
use std::time::Instant;

use inkra_text::{Bias, Buffer, ByteOffset, Hunk, diff};
use proptest::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

// Some share a base or all but one ZWJ member, so an edit can fall inside a grapheme.
const FRAGMENTS: [&str; 25] = [
    "a",
    "fox",
    "the",
    "_x1",
    " ",
    "  ",
    "\n",
    "\n\n",
    "\n\n\n",
    ".",
    "!!",
    "#",
    "é",
    "café",
    "e\u{301}",
    "e\u{300}",
    "a\u{301}",
    "\u{301}",
    "日本",
    "🎉",
    "👨\u{200d}👩\u{200d}👧",
    "👨\u{200d}👩\u{200d}👦",
    "\u{200d}",
    "\t",
    ",",
];

fn fragment() -> impl Strategy<Value = &'static str> {
    prop::sample::select(FRAGMENTS.as_slice())
}

/// Mostly an edited copy, so hunks sit between unchanged text; sometimes two unrelated texts.
fn old_and_new() -> impl Strategy<Value = (String, String)> {
    let edited =
        prop::collection::vec((fragment(), 0u8..6, fragment()), 0..120).prop_map(|parts| {
            let (mut old, mut new) = (String::new(), String::new());
            for (kept, action, other) in parts {
                match action {
                    0 => old.push_str(kept),
                    1 => {
                        old.push_str(kept);
                        new.push_str(other);
                    }
                    2 => new.push_str(other),
                    _ => {
                        old.push_str(kept);
                        new.push_str(kept);
                    }
                }
            }
            (old, new)
        });
    let unrelated = (
        prop::collection::vec(fragment(), 0..60).prop_map(|parts| parts.concat()),
        prop::collection::vec(fragment(), 0..60).prop_map(|parts| parts.concat()),
    );
    prop_oneof![4 => edited, 1 => unrelated]
}

fn grapheme_boundary(text: &str, offset: usize) -> bool {
    offset == text.len()
        || text
            .grapheme_indices(true)
            .any(|(start, _)| start == offset)
}

/// How many bytes a hunk adds to the text, negative when it removes more than it inserts.
fn growth(range: &Range<ByteOffset>, text: &str) -> isize {
    text.len().cast_signed() - (range.end - range.start).cast_signed()
}

// Strict: at a hunk's start a `Left` anchor stays before the replacement, off its old text.
fn outside_every_hunk(offset: usize, hunks: &[Hunk]) -> bool {
    hunks
        .iter()
        .all(|hunk| offset < hunk.range.start.get() || offset > hunk.range.end.get())
}

proptest! {
    #[test]
    fn applying_the_diff_to_old_yields_new((old, new) in old_and_new()) {
        let mut buffer = Buffer::new(old.as_str());
        let diff = diff(buffer.version(), &old, &new);
        for pair in diff.hunks().windows(2) {
            prop_assert!(pair[0].range.end <= pair[1].range.start, "hunks overlap: {:?}", pair);
        }

        prop_assert!(buffer.apply_diff(Instant::now(), &diff));
        prop_assert_eq!(buffer.text(), new.as_str());
    }

    #[test]
    fn every_hunk_starts_and_ends_on_graphemes_of_both_texts((old, new) in old_and_new()) {
        let mut shift = 0isize;
        for Hunk { range, text } in diff(0, &old, &new).hunks() {
            prop_assert!(grapheme_boundary(&old, range.start.get()), "old {:?} at {:?}", old, range);
            prop_assert!(grapheme_boundary(&old, range.end.get()), "old {:?} at {:?}", old, range);
            let start = range.start.get().strict_add_signed(shift);
            prop_assert!(grapheme_boundary(&new, start), "new {:?} at {}", new, start);
            prop_assert!(grapheme_boundary(&new, start + text.len()), "new {:?} at {}", new, start);
            shift += growth(range, text);
        }
    }

    #[test]
    fn one_undo_after_the_apply_restores_old((old, new) in old_and_new()) {
        let mut buffer = Buffer::new(old.as_str());
        let diff = diff(buffer.version(), &old, &new);
        prop_assert!(buffer.apply_diff(Instant::now(), &diff));
        buffer.undo();
        prop_assert_eq!(buffer.text(), old.as_str());
    }

    #[test]
    fn an_anchor_in_unchanged_text_stays_on_the_same_text((old, new) in old_and_new()) {
        let mut buffer = Buffer::new(old.as_str());
        let diff = diff(buffer.version(), &old, &new);
        let hunks = diff.hunks();
        let anchored: Vec<_> = old
            .char_indices()
            .map(|(offset, _)| offset)
            .filter(|&offset| outside_every_hunk(offset, hunks))
            .enumerate()
            .map(|(index, offset)| {
                let bias = if index % 2 == 0 { Bias::Left } else { Bias::Right };
                (offset, buffer.anchor(ByteOffset::new(offset), bias))
            })
            .collect();

        prop_assert!(buffer.apply_diff(Instant::now(), &diff));

        for (offset, anchor) in anchored {
            let shift: isize = hunks
                .iter()
                .filter(|hunk| hunk.range.end.get() < offset)
                .map(|hunk| growth(&hunk.range, &hunk.text))
                .sum();
            let resolved = buffer.resolve(anchor).get();
            prop_assert_eq!(resolved.cast_signed(), offset.cast_signed() + shift);
            prop_assert_eq!(new[resolved..].chars().next(), old[offset..].chars().next());
        }
    }

    #[test]
    fn a_mapped_offset_equals_an_anchor_across_the_apply(
        (old, new) in old_and_new(),
        pick in any::<prop::sample::Index>(),
        left in any::<bool>(),
    ) {
        let offsets: Vec<usize> = old.char_indices().map(|(offset, _)| offset).chain([old.len()]).collect();
        let offset = ByteOffset::new(offsets[pick.index(offsets.len())]);
        let bias = if left { Bias::Left } else { Bias::Right };
        let mut buffer = Buffer::new(old.as_str());
        let diff = diff(buffer.version(), &old, &new);
        let anchor = buffer.anchor(offset, bias);

        prop_assert!(buffer.apply_diff(Instant::now(), &diff));

        prop_assert_eq!(diff.map(offset, bias), buffer.resolve(anchor));
    }

    /// Checked by arithmetic, not against anchors: they share `transform` with `Diff::map`.
    #[test]
    fn a_mapped_offset_moves_by_the_hunks_before_it_or_to_its_hunks_edge(
        (old, new) in old_and_new(),
    ) {
        let diff = diff(0, &old, &new);
        let hunks = diff.hunks();
        let offsets = old.char_indices().map(|(offset, _)| offset).chain([old.len()]);
        for offset in offsets {
            for bias in [Bias::Left, Bias::Right] {
                let mapped = diff.map(ByteOffset::new(offset), bias).get();
                let touched = hunks
                    .iter()
                    .position(|hunk| hunk.range.start.get() <= offset && offset <= hunk.range.end.get());
                let Some(index) = touched else {
                    let shift: isize = hunks
                        .iter()
                        .filter(|hunk| hunk.range.end.get() < offset)
                        .map(|hunk| growth(&hunk.range, &hunk.text))
                        .sum();
                    prop_assert_eq!(mapped.cast_signed(), offset.cast_signed() + shift);
                    prop_assert_eq!(new[mapped..].chars().next(), old[offset..].chars().next());
                    continue;
                };
                let hunk = &hunks[index];
                let shift: isize = hunks[..index]
                    .iter()
                    .map(|hunk| growth(&hunk.range, &hunk.text))
                    .sum();
                let start = hunk.range.start.get().strict_add_signed(shift);
                let end = start + hunk.text.len();
                // The end of a removal is unchanged text after the hunk, so the bias does not apply.
                let at_end = offset == hunk.range.end.get() && hunk.range.start < hunk.range.end;
                let expected = match bias {
                    _ if at_end => end,
                    Bias::Left => start,
                    Bias::Right => end,
                };
                prop_assert_eq!(mapped, expected, "{:?} at {} in {:?}", bias, offset, hunk);
            }
        }
    }
}
