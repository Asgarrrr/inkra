//! `⌘L` on a selection: one batch over every line it touches (task boxes spec, D6, amended for T4b).

use std::ops::Range;

use inkra_text::{Bias, ByteOffset, Diff, Version};
use proptest::prelude::*;
use proptest::sample::Index;

use super::cycle_tasks;
use super::tests::{apply, free_line, in_block, task_at};
use crate::parse;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

/// `text` after `⌘L` with `range` selected.
fn selected(text: &str, range: Range<usize>) -> String {
    apply(
        text,
        &cycle_tasks(text, &parse(text), at(range.start)..at(range.end)),
    )
}

/// `text` after `⌘A ⌘L`.
fn all(text: &str) -> String {
    selected(text, 0..text.len())
}

#[test]
fn every_row_of_the_table_cycles_in_a_selection() {
    for (text, want) in [
        ("- [ ] a\n1. [ ] b", "- [x] a\n1. [x] b"),
        ("- [x] a\n- [X] b", "- [ ] a\n- [ ] b"),
        ("- a\n* b\n1. c", "- [ ] a\n* [ ] b\n1. [ ] c"),
        ("a\n> b\n  c", "- [ ] a\n> - [ ] b\n  - [ ] c"),
        ("-\n- b", "- [ ] \n- [ ] b"),
    ] {
        assert_eq!(all(text), want, "{text:?}");
    }
}

/// A line no task holds takes the target's checkbox, ticked under a first line being ticked.
#[test]
fn a_plain_line_takes_the_targets_checkbox() {
    assert_eq!(all("- [ ] a\nb\n- c"), "- [x] a\n- [x] b\n- [x] c");
    assert_eq!(all("- [ ] a\n> b"), "- [x] a\n> - [x] b");
}

#[test]
fn a_mixed_selection_lands_on_the_first_lines_next_state() {
    for (text, want) in [
        ("- a\n- [x] b\nc", "- [ ] a\n- [ ] b\n- [ ] c"),
        ("- [x] a\n- [ ] b\nc", "- [ ] a\n- [ ] b\n- [ ] c"),
        ("- [ ] a\n- [x] b\n- [ ] c", "- [x] a\n- [x] b\n- [x] c"),
    ] {
        assert_eq!(all(text), want, "{text:?}");
    }
}

/// Only the first cycled line sets the target: a skipped first line does not.
#[test]
fn the_first_cycled_line_sets_the_target() {
    let text = "```\n- [ ] a\n```\n- [ ] b\n- c";
    assert_eq!(all(text), "```\n- [ ] a\n```\n- [x] b\n- [x] c");
}

/// (H): a selection ending at a later line's column 0 leaves that line.
#[test]
fn a_selection_ending_at_column_0_drops_that_line() {
    let text = "a\nb\nc";
    assert_eq!(selected(text, 0..4), "- [ ] a\n- [ ] b\nc");
    assert_eq!(selected(text, 0..5), "- [ ] a\n- [ ] b\n- [ ] c");
    assert_eq!(selected(text, 1..2), "- [ ] a\nb\nc");
    assert_eq!(selected(text, 2..2), "a\n- [ ] b\nc", "a caret at column 0");
    assert_eq!(all("a\nb\n"), "- [ ] a\n- [ ] b\n", "the empty last line");
}

#[test]
fn a_selection_skips_blank_lines() {
    assert_eq!(all("a\n\n  \n\t\nb"), "- [ ] a\n\n  \n\t\n- [ ] b");
    assert_eq!(selected("a\n\nb", 1..3), "- [ ] a\n\nb");
}

#[test]
fn a_selection_skips_code_front_matter_tables_and_html() {
    for (text, want) in [
        ("a\n```\nb\n```\nc", "- [ ] a\n```\nb\n```\n- [ ] c"),
        ("a\n\n    b\n\nc", "- [ ] a\n\n    b\n\n- [ ] c"),
        ("---\nt: x\n---\na", "---\nt: x\n---\n- [ ] a"),
        (
            "a\n\n| b |\n|---|\n| c |\n\nd",
            "- [ ] a\n\n| b |\n|---|\n| c |\n\n- [ ] d",
        ),
        (
            "a\n\n<div>\nb\n</div>\n\nc",
            "- [ ] a\n\n<div>\nb\n</div>\n\n- [ ] c",
        ),
    ] {
        assert_eq!(all(text), want, "{text:?}");
    }
}

/// (I): `⌘A ⌘L` leaves the YAML as it is.
#[test]
fn select_all_leaves_the_front_matter() {
    let text = "---\ntitle: a\ntags: [b]\n---\n\nfirst\nsecond\n\n- [x] done\n";
    assert_eq!(
        all(text),
        "---\ntitle: a\ntags: [b]\n---\n\n- [ ] first\n- [ ] second\n\n- [ ] done\n"
    );
}

/// The batch is judged whole: a new item makes the next line a nested task, which it cannot be
/// alone.
#[test]
fn a_new_item_lets_the_next_line_nest() {
    let text = "a\n    b";
    assert_eq!(all(text), "- [ ] a\n    - [ ] b");
    assert_eq!(selected(text, 2..text.len()), text, "alone, `b` is lazy");
}

/// After a new item `2.` starts a list of its own; alone, it is lazy and takes `- [ ] `.
#[test]
fn a_new_item_lets_the_next_number_start_a_list() {
    let text = "a\n2. b";
    assert_eq!(selected(text, 2..2), "a\n- [ ] 2. b", "alone");
    assert_eq!(all(text), "- [ ] a\n2. [ ] b");
}

/// A line whose first candidate does not land takes its second, and the rest keeps theirs.
#[test]
fn a_line_whose_bullet_does_not_land_takes_the_second_candidate() {
    let text = "x\n2. b\nc";
    assert_eq!(selected(text, 2..text.len()), "x\n- [ ] 2. b\n- [ ] c");
}

/// A line neither candidate makes a task drops out, and the rest still lands.
#[test]
fn a_line_no_candidate_lands_drops_out_of_the_batch() {
    let text = "x\n    2. b\nc";
    assert_eq!(selected(text, 2..text.len()), "x\n    2. b\n- [ ] c");
}

/// Every failing line moves on in the same round, so `c`, lazy under the failing `2. [ ] b`,
/// runs out before `- [ ] 2. b` would let it nest (spec, Deviations, T4b).
#[test]
fn a_line_that_runs_out_before_its_neighbour_settles_drops_out() {
    let text = "x\n2. b\n    c";
    assert_eq!(selected(text, 2..text.len()), "x\n- [ ] 2. b\n    c");
}

/// The line indices `start..end` touches by (H).
fn touched_lines(text: &str, start: usize, end: usize) -> Range<usize> {
    let index = |at: usize| text[..at].matches('\n').count();
    let column_0 = end > start && text[..end].ends_with('\n');
    index(start)..index(end) + usize::from(!column_0)
}

/// The line starting at `start`, without its newline.
fn line_from(text: &str, start: usize) -> Range<usize> {
    start
        ..text[start..]
            .find('\n')
            .map_or(text.len(), |len| start + len)
}

fn starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

proptest! {
    /// Every changed line reparses as a task in the first cycled line's next state, every other
    /// line is byte-identical, and the mapped selection touches the same lines.
    #[test]
    fn a_batch_lands_every_changed_line(
        lines in prop::collection::vec(free_line(), 1..8),
        newline in any::<bool>(),
        ends in any::<(Index, Index)>(),
    ) {
        let text = lines.join("\n") + if newline { "\n" } else { "" };
        let points: Vec<usize> = (0..=text.len()).filter(|&at| text.is_char_boundary(at)).collect();
        let [a, b] = [ends.0, ends.1].map(|end| points[end.index(points.len())]);
        let (start, end) = (a.min(b), a.max(b));
        let hunks = cycle_tasks(&text, &parse(&text), at(start)..at(end));
        let diff = Diff::new(Version::default(), hunks.clone());

        let starts = starts(&text);
        let line = |index: usize| line_from(&text, starts[index]);
        let touched = touched_lines(&text, start, end);
        let multi = touched.len() > 1;
        let cycled: Vec<usize> = touched
            .clone()
            .filter(|&index| {
                let line = line(index);
                let blank = text[line.clone()].bytes().all(|byte| matches!(byte, b' ' | b'\t'));
                !(multi && blank || in_block(&text, line.start, line.end))
            })
            .collect();
        let target = cycled.first().is_some_and(|&first| task_at(&text, starts[first]) == Some(false));

        let after = apply(&text, &hunks);
        let mut changed = vec![false; starts.len()];
        for hunk in &hunks {
            let index = text[..hunk.range.start.get()].matches('\n').count();
            let line = line(index);
            prop_assert!(cycled.contains(&index), "{:?} edits line {}", text, index);
            prop_assert!(line.start <= hunk.range.start.get() && hunk.range.end.get() <= line.end);
            prop_assert!(!hunk.text.contains('\n'));
            changed[index] = true;
        }
        let after_starts = self::starts(&after);
        prop_assert_eq!(after_starts.len(), starts.len());
        for (index, &changed) in changed.iter().enumerate() {
            let old = line(index);
            if changed {
                prop_assert_eq!(
                    task_at(&after, after_starts[index]),
                    Some(target),
                    "{:?} became {:?} at line {}", text, after, index
                );
            } else {
                prop_assert_eq!(&after[line_from(&after, after_starts[index])], &text[old]);
                if cycled.contains(&index) && task_at(&text, starts[index]) == Some(target) {
                    prop_assert_eq!(task_at(&after, after_starts[index]), Some(target));
                }
            }
        }

        let (mapped_start, mapped_end) = if start == end {
            let caret = diff.map(at(start), Bias::Right).get();
            (caret, caret)
        } else {
            let mapped_start = diff.map(at(start), Bias::Right).get();
            (mapped_start, diff.map(at(end), Bias::Left).get().max(mapped_start))
        };
        for mapped in [mapped_start, mapped_end] {
            prop_assert!(mapped <= after.len() && after.is_char_boundary(mapped));
        }
        prop_assert_eq!(touched_lines(&after, mapped_start, mapped_end), touched);
    }
}
