use std::fmt::Debug;
use std::ops::Range;

use inkra_text::{ByteOffset, Splice};
use proptest::prelude::*;

use super::changed;
use crate::{
    CodeBlock, Conceal, Gutter, Heading, HeadingLevel, ItemPart, Markdown, Span, Styles, Task,
    WidthKey, parse,
};

/// `old` with `removed` bytes at `at` replaced by `inserted`, and what `changed` answers for it,
/// sorted and merged.
fn answered(old: &str, at: usize, removed: usize, inserted: &str) -> Vec<(usize, usize)> {
    let mut new = old.to_owned();
    new.replace_range(at..at + removed, inserted);
    let edit = Splice {
        at: ByteOffset::new(at),
        removed_len: removed,
        inserted_len: inserted.len(),
    };
    merged(changed(&parse(old), &parse(&new), edit))
}

/// Each answered range as its start and end, sorted and merged.
fn merged(ranges: Vec<Range<ByteOffset>>) -> Vec<(usize, usize)> {
    let mut ranges: Vec<_> = ranges
        .into_iter()
        .map(|range| (range.start.get(), range.end.get()))
        .collect();
    ranges.sort_unstable();
    ranges.dedup_by(|next, kept| {
        let meets = next.0 <= kept.1;
        if meets {
            kept.1 = kept.1.max(next.1);
        }
        meets
    });
    ranges
}

#[test]
fn typing_in_plain_prose_changes_only_the_edit() {
    let text = "# Head\n\nsome prose\n\n> quote\n";

    assert_eq!(answered(text, 12, 0, "x"), [(12, 13)]);
    assert_eq!(answered(text, 12, 1, "B"), [(12, 13)]);
}

#[test]
fn typing_after_bold_leaves_the_bold_out() {
    assert_eq!(answered("**a** b\n\nc", 5, 0, "x"), [(5, 6)]);
}

#[test]
fn typing_inside_a_block_changes_only_the_edit() {
    let text = "a\n\n```\nlet x;\n```\n\nb";

    assert_eq!(answered(text, 9, 0, "y"), [(9, 10)]);
}

#[test]
fn closing_a_block_leaves_its_kept_lines_out() {
    let answer = answered("```\na\nb\n``\nc", 10, 0, "`");

    assert!(
        answer.iter().all(|&(start, end)| end < 4 || 7 < start),
        "{answer:?}"
    );
    assert!(answer.iter().any(|&(start, _)| start == 0), "{answer:?}");
}

#[test]
fn a_setext_underline_reaches_the_line_above() {
    assert_eq!(answered("a\n\nTitle\nx\n\nb", 9, 1, "="), [(3, 8), (9, 10)]);
}

#[test]
fn a_heading_level_change_is_a_change() {
    assert_eq!(answered("# a\n\nb", 0, 0, "#"), [(0, 4)]);
}

#[test]
fn a_gutter_gaining_a_quote_is_a_change() {
    assert_eq!(answered("> a\n> b\n\nc", 4, 0, ">"), [(4, 7)]);
}

/// Nine items, the eighth with a child, then a paragraph.
const NINE: &str = "1. a\n2. b\n3. c\n4. d\n5. e\n6. f\n7. g\n8. h\n   - x\n9. i\n\nz";

/// Only the lines whose gutter changed (task boxes spec, T6c1): the bullet's line, not `- b`.
#[test]
fn a_bullet_turned_into_a_number_dirties_its_line_alone() {
    assert_eq!(answered("- a\n\n- b\n\nz", 0, 1, "1."), [(0, 3)]);
}

/// `- b` becomes `- a`'s child: its own gutter changes, `- a`'s does not.
#[test]
fn two_blanks_before_a_bullet_dirty_its_line_alone() {
    assert_eq!(answered("- a\n- b\n\nz", 4, 0, "  "), [(4, 8)]);
}

/// The blank line ends the item: both lazy lines lose their gutter, and `- a` keeps its own.
#[test]
fn a_blank_line_that_ends_an_item_dirties_the_lines_it_releases() {
    assert_eq!(answered("- a\nb\nc\n\nz", 4, 0, "\n"), [(4, 5), (7, 7)]);
}

/// The new list's keys come first in `width_keys`, so the old list's item parts index other keys.
#[test]
fn an_ordered_list_typed_above_another_leaves_its_lines_clean() {
    assert_eq!(answered("a\n\n1. b\n   c\n", 0, 0, "1. x\n\n"), [(0, 6)]);
}

/// `10.` widens the list's width key: every item part of the list and its child, each once, and
/// not the paragraph after it.
#[test]
fn a_tenth_item_dirties_every_line_of_its_list() {
    let lines = (0..8).map(|item| (5 * item, 5 * item + 3));
    let mut list: Vec<_> = lines.collect();
    list.extend([(40, 45), (47, 50), (51, 57)]);
    assert_eq!(answered(NINE, 51, 0, "\n10. j"), list);
}

#[test]
fn a_letter_in_the_eighth_item_dirties_its_line_alone() {
    assert_eq!(answered(NINE, 39, 0, "x"), [(39, 40)]);
}

#[test]
fn a_link_losing_its_target_reaches_its_old_markers() {
    assert_eq!(answered("a [t](u) b\n\nc", 5, 3, ""), [(2, 5)]);
}

#[test]
fn an_unchanged_code_block_on_either_side_stays_out() {
    assert_eq!(answered("```rs\nx\n```\n\nab", 14, 0, "y"), [(14, 15)]);
    assert_eq!(answered("ab\n\n```rs\nx\n```", 1, 0, "y"), [(1, 2)]);
}

fn offsets(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}

/// What `changed` answers for `old` and `new` around `removed` bytes at `at` replaced by
/// `inserted` ones, sorted and merged.
fn answered_between(
    old: &Markdown,
    new: &Markdown,
    at: usize,
    removed: usize,
    inserted: usize,
) -> Vec<(usize, usize)> {
    let edit = Splice {
        at: ByteOffset::new(at),
        removed_len: removed,
        inserted_len: inserted,
    };
    merged(changed(old, new, edit))
}

/// What `changed` answers for one byte typed at 20, past every item of `old` and `new`.
fn typed_after(old: &Markdown, new: &Markdown) -> Vec<(usize, usize)> {
    answered_between(old, new, 20, 0, 1)
}

fn spans(range: Range<usize>) -> Markdown {
    Markdown {
        spans: vec![Span {
            range: offsets(range),
            styles: Styles::NONE,
        }],
        ..Markdown::default()
    }
}

#[test]
fn an_item_over_an_insertion_that_keeps_its_length_is_a_change() {
    assert_eq!(
        answered_between(&spans(0..5), &spans(0..5), 2, 0, 1),
        [(0, 6)]
    );
}

#[test]
fn an_item_past_a_deletion_that_lands_elsewhere_is_a_change() {
    assert_eq!(
        answered_between(&spans(10..12), &spans(12..14), 2, 2, 0),
        [(2, 2), (8, 10), (12, 14)]
    );
}

#[test]
fn a_conceal_added_before_another_leaves_it_out() {
    let conceal = |element, marker| Conceal {
        element: offsets(element),
        marker: offsets(marker),
        replacement: "",
    };
    // A closing marker: its element starts before the new conceal's.
    let closing = conceal(0..14, 10..11);
    let old = Markdown {
        conceals: vec![closing.clone()],
        ..Markdown::default()
    };
    let new = Markdown {
        conceals: vec![conceal(2..6, 2..3), closing],
        ..Markdown::default()
    };

    assert_eq!(typed_after(&old, &new), [(2, 3), (20, 21)]);
}

#[test]
fn a_block_that_ends_sooner_reaches_its_opening_line_and_the_lines_it_releases() {
    let block = |end| Markdown {
        code_blocks: vec![CodeBlock {
            range: offsets(0..end),
            language: None,
            shape: 0,
        }],
        ..Markdown::default()
    };

    assert_eq!(
        typed_after(&block(15), &block(8)),
        [(0, 0), (8, 15), (20, 21)]
    );
}

#[test]
fn a_conceal_before_the_edit_that_changes_any_field_is_a_change() {
    let conceal = |element, marker, replacement| Conceal {
        element: offsets(element),
        marker: offsets(marker),
        replacement,
    };
    let old = Markdown {
        conceals: vec![conceal(0..6, 2..4, "")],
        ..Markdown::default()
    };
    for new in [
        conceal(0..7, 2..4, ""),
        conceal(0..6, 2..5, ""),
        conceal(0..6, 2..4, "•"),
    ] {
        let new = Markdown {
            conceals: vec![new],
            ..Markdown::default()
        };

        assert_eq!(typed_after(&old, &new)[0].0, 2, "{new:?}");
    }
}

#[test]
fn a_gutter_before_the_edit_that_changes_any_field_is_a_change() {
    let task = |checkbox, checked| {
        Some(Task {
            checkbox: offsets(checkbox),
            checked,
        })
    };
    let key = |digits| WidthKey {
        digits,
        delimiter: b'.',
    };
    let item = |start, indent, numbers: &'static [u8], task| Some((start, indent, numbers, task));
    let gutter = |end, quotes: &[usize], depth, item: Option<(_, _, &[u8], _)>| Markdown {
        gutters: vec![Gutter {
            range: offsets(0..end),
            quotes: quotes.iter().copied().map(ByteOffset::new).collect(),
            depth,
            item: item.clone().map(|(start, indent, numbers, task)| ItemPart {
                start: ByteOffset::new(start),
                indent,
                keys: 0..numbers.len(),
                task,
            }),
        }],
        width_keys: item.map_or_else(Vec::new, |(_, _, numbers, _)| {
            numbers.iter().copied().map(key).collect()
        }),
        ..Markdown::default()
    };
    let old = gutter(12, &[0, 2], 2, item(4, 1, &[1], task(6..9, false)));
    for new in [
        gutter(10, &[0, 2], 2, item(4, 1, &[1], task(6..9, false))),
        gutter(12, &[0, 1], 2, item(4, 1, &[1], task(6..9, false))),
        gutter(12, &[0], 2, item(4, 1, &[1], task(6..9, false))),
        gutter(12, &[0, 2], 3, item(4, 1, &[1], task(6..9, false))),
        gutter(12, &[0, 2], 2, item(4, 1, &[1], task(6..9, true))),
        gutter(12, &[0, 2], 2, item(4, 1, &[1], task(7..10, false))),
        gutter(12, &[0, 2], 2, item(4, 1, &[1], None)),
        gutter(12, &[0, 2], 2, item(3, 1, &[1], task(6..9, false))),
        gutter(12, &[0, 2], 2, item(4, 2, &[1], task(6..9, false))),
        gutter(12, &[0, 2], 2, item(4, 1, &[2], task(6..9, false))),
        gutter(12, &[0, 2], 2, item(4, 1, &[1, 1], task(6..9, false))),
        gutter(12, &[0, 2], 2, None),
    ] {
        assert_eq!(typed_after(&old, &new)[0].0, 0, "{new:?}");
    }
}

#[test]
fn a_heading_before_the_edit_that_changes_level_is_a_change() {
    let heading = |level| Markdown {
        headings: vec![Heading {
            range: offsets(0..5),
            level,
            marker: Some(offsets(0..2)),
        }],
        ..Markdown::default()
    };

    assert_eq!(
        typed_after(&heading(HeadingLevel::H1), &heading(HeadingLevel::H2)),
        [(0, 5), (20, 21)]
    );
}

#[test]
fn a_heading_that_loses_lines_reaches_them() {
    let heading = |end| Markdown {
        headings: vec![Heading {
            range: offsets(0..end),
            level: HeadingLevel::H1,
            marker: None,
        }],
        ..Markdown::default()
    };

    assert_eq!(typed_after(&heading(10), &heading(4)), [(0, 10), (20, 21)]);
}

/// What the layout reads on `line` (`app`'s `layout/shape.rs`), offsets relative to the line's start:
/// the spans, conceals and gutters that start on it, the heading and block that hold it, and their
/// marker and language when on it. An item reaching past the line shows only its part on the line,
/// and which ends it crosses.
fn on_line(markdown: &Markdown, line: &Range<usize>) -> Vec<String> {
    let starts = |offset: ByteOffset| line.start <= offset.get() && offset.get() <= line.end;
    let holds =
        |range: &Range<ByteOffset>| range.start.get() <= line.end && line.start <= range.end.get();
    let rel = |offset: ByteOffset| offset.get().cast_signed() - line.start.cast_signed();
    let span = |range: &Range<ByteOffset>| rel(range.start)..rel(range.end);
    let clip = |range: &Range<ByteOffset>| {
        let (start, end) = (range.start.get(), range.end.get());
        (
            start.max(line.start) - line.start..end.min(line.end) - line.start,
            start < line.start,
            end > line.end,
        )
    };
    let block = markdown
        .code_blocks
        .iter()
        .find(|block| holds(&block.range));
    let mut items = Vec::new();
    let mut push = |item: &dyn Debug| items.push(format!("{item:?}"));
    for item in markdown
        .spans
        .iter()
        .filter(|item| starts(item.range.start))
    {
        push(&(span(&item.range), item.styles));
    }
    for item in markdown
        .conceals
        .iter()
        .filter(|item| starts(item.marker.start))
    {
        let labels = block.is_some_and(|block| block.range == item.element);
        push(&(
            span(&item.marker),
            item.replacement,
            clip(&item.element),
            labels,
        ));
    }
    for item in markdown
        .gutters
        .iter()
        .filter(|item| starts(item.range.start))
    {
        let quotes: Vec<_> = item.quotes.iter().map(|&quote| rel(quote)).collect();
        let part = item.item.as_ref().map(|part| {
            let task = part
                .task
                .as_ref()
                .map(|task| (span(&task.checkbox), task.checked));
            let numbers = &markdown.width_keys[part.keys.clone()];
            (rel(part.start), part.indent, numbers, task)
        });
        push(&(span(&item.range), quotes, item.depth, part));
    }
    if let Some(item) = markdown.headings.iter().find(|item| holds(&item.range)) {
        let marker = item.marker.as_ref().filter(|marker| starts(marker.start));
        push(&(clip(&item.range), item.level, marker.map(span)));
    }
    if let Some(item) = block {
        let language = item.language.as_ref().filter(|_| starts(item.range.start));
        push(&(clip(&item.range), language.map(span)));
    }
    items
}

/// Each line of `text`, without its `\n`.
fn lines(text: &str) -> Vec<Range<usize>> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let range = start..start + line.len();
            start = range.end + 1;
            range
        })
        .collect()
}

fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("```\n".to_string()),
        Just("```rs\n".to_string()),
        Just("# ".to_string()),
        Just("## ".to_string()),
        Just("> ".to_string()),
        Just("> > ".to_string()),
        Just("- ".to_string()),
        Just("- [ ] ".to_string()),
        Just("- [x] ".to_string()),
        Just("1. ".to_string()),
        Just("10. ".to_string()),
        Just("  ".to_string()),
        Just("===\n".to_string()),
        Just("---\n".to_string()),
        Just("**".to_string()),
        Just("*".to_string()),
        Just("`".to_string()),
        Just("[t](u)".to_string()),
        Just("[[p|a]]".to_string()),
        Just("    ".to_string()),
        "[a-zé ]{1,6}".prop_map(String::from),
        Just("\n".to_string()),
        Just("\n\n".to_string()),
    ]
}

/// A text and one edit in it: `removed` bytes at `at`, both on char boundaries, and the text put there.
fn edited() -> impl Strategy<Value = (String, usize, usize, String)> {
    (
        prop::collection::vec(fragment(), 0..24),
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        prop::collection::vec(fragment(), 0..3),
    )
        .prop_map(|(old, at, removed, inserted)| {
            let old = old.concat();
            let at = old.floor_char_boundary(at.index(old.len() + 1));
            let end = old.floor_char_boundary(at + removed.index(old.len() - at + 1).min(8));
            (old, at, end.max(at) - at, inserted.concat())
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn every_line_outside_the_answer_reads_what_it_read_before(
        (old, at, removed, inserted) in edited()
    ) {
        let mut new = old.clone();
        new.replace_range(at..at + removed, &inserted);
        let answer = answered(&old, at, removed, &inserted);
        prop_assert!(
            answer.iter().any(|&(start, end)| start <= at && at + inserted.len() <= end),
            "{:?} misses the edit", answer
        );
        let (before, after) = (parse(&old), parse(&new));
        for line in lines(&new) {
            if answer.iter().any(|&(start, end)| line.start <= end && start <= line.end) {
                continue;
            }
            let was = if line.end < at {
                line.clone()
            } else {
                line.start - inserted.len() + removed..line.end - inserted.len() + removed
            };
            prop_assert_eq!(
                on_line(&before, &was),
                on_line(&after, &line),
                "line {:?} of {:?}, from {:?}", line, new, old
            );
        }
    }
}
