//! `⇥` and `⇧⇥` per case, each written `text` with `|` for the caret, or two `|` around a
//! selection (list editing spec, D4, D7, D8).

use std::ops::Range;

use inkra_text::ByteOffset;

use super::{indent_items, outdent_items};
use crate::structure::FALLBACKS;
use crate::testing::edited;
use crate::{ListEdit, Markdown, parse};

type Command = fn(&str, &Markdown, Range<ByteOffset>) -> Option<ListEdit>;

/// `case`'s text after `command`, with `|` at a caret's landing; `None` when no item is touched.
fn run(command: Command, case: &str) -> Option<String> {
    let start = case.find('|').expect("a caret");
    let text = case.replacen('|', "", 1);
    let end = text.find('|').unwrap_or(start);
    let text = text.replacen('|', "", 1);
    let edit = command(&text, &parse(&text), at(start)..at(end))?;
    let (mut out, caret) = edited(&text, &edit);
    if start == end {
        out.insert(caret, '|');
    }
    Some(out)
}

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

fn tab(cases: &[(&str, &str)]) {
    for &(case, want) in cases {
        assert_eq!(run(indent_items, case).as_deref(), Some(want), "{case:?}");
    }
}

fn shift_tab(cases: &[(&str, &str)]) {
    for &(case, want) in cases {
        assert_eq!(run(outdent_items, case).as_deref(), Some(want), "{case:?}");
    }
}

#[test]
fn tab_nests_an_item_under_its_previous_sibling() {
    tab(&[
        ("- a\n- |b", "- a\n  - |b"),
        ("- a\n|- b", "- a\n  |- b"),
        ("* a\n* b|", "* a\n  * b|"),
        ("-   a\n- b|", "-   a\n    - b|"),
        ("> - a\n> - b|", "> - a\n>   - b|"),
        ("- a\n  - b\n  - c|", "- a\n  - b\n    - c|"),
        ("- [ ] a\n- [x] b|", "- [ ] a\n  - [x] b|"),
    ]);
}

/// The new parent's content column, wherever the run's marker stood: a nested `10.` by four.
#[test]
fn tab_lands_the_marker_on_the_new_parents_content_column() {
    tab(&[
        ("10. a\n11. b|", "10. a\n    1. b|"),
        ("- a\n - b|", "- a\n  - b|"),
        ("1) a\n2) b|", "1) a\n   1) b|"),
    ]);
}

/// Its children, held lines, lazy lines and code move with it.
#[test]
fn tab_moves_the_whole_block() {
    tab(&[
        ("- a\n- b|\n  - c", "- a\n  - b|\n    - c"),
        ("- a\n- b|\n  c\n\n  d", "- a\n  - b|\n    c\n\n    d"),
        ("- a\n- b|\nlazy", "- a\n  - b|\n  lazy"),
        (
            "- a\n- b|\n  ```\n  x\n  ```",
            "- a\n  - b|\n    ```\n    x\n    ```",
        ),
        ("> - a\n> - b|\n>   c", "> - a\n>   - b|\n>     c"),
    ]);
}

#[test]
fn tab_writes_after_the_leading_tabs() {
    tab(&[("- a\n\t- b\n\t- c|", "- a\n\t- b\n\t  - c|")]);
}

/// A run moves as one; a line that gives no item of its own moves with its holder.
#[test]
fn tab_on_a_selection_moves_its_runs() {
    tab(&[
        ("- a\n- |b\n- c|", "- a\n  - b\n  - c"),
        ("- a\n- |b\n  held\n- c|", "- a\n  - b\n    held\n  - c"),
        ("1. a\n2. |b\n3. c|", "1. a\n   1. b\n   2. c"),
        ("- a\n- |b\n  - x\n- c|", "- a\n  - b\n    - x\n  - c"),
        ("- a\n- |b\n|- c", "- a\n  - b\n- c"),
    ]);
}

/// A first item cannot become a child: nothing is written.
#[test]
fn tab_on_a_first_item_writes_nothing() {
    for case in [
        "- a|",
        "- a\n  - b|",
        "> - a\n\n- b|",
        "para\n\n- b|",
        "- - a|",
    ] {
        assert_eq!(run(indent_items, case), Some(case.to_owned()), "{case:?}");
    }
}

/// The run joins the new parent's last child list, its kind and its numbers; else a bullet keeps
/// its byte and a number restarts at 1.
#[test]
fn tab_takes_the_kind_of_the_new_parents_last_child() {
    tab(&[
        ("- a\n  * x\n- b|", "- a\n  * x\n  * b|"),
        ("- a\n  1. x\n  2. y\n- b|", "- a\n  1. x\n  2. y\n  3. b|"),
        ("- a\n  01. x\n- b|", "- a\n  01. x\n  02. b|"),
        ("1. a\n   - x\n2. b|", "1. a\n   - x\n   - b|"),
        ("- a\n  - x\n\n- b|", "- a\n  - x\n\n  - b|"),
        ("3. a\n4. b|", "3. a\n   1. b|"),
    ]);
    // A paragraph after the last child list ends it: the run starts its own.
    tab(&[("- a\n  * x\n\n  p\n- b|", "- a\n  * x\n\n  p\n  - b|")]);
}

/// A marker whose width changes shifts its item's other lines (D7).
#[test]
fn tab_shifts_the_lines_of_a_marker_that_widens() {
    tab(&[
        (
            "- a\n\n  9. x\n- b|\n  c\n  - d",
            "- a\n\n  9. x\n  10. b|\n      c\n      - d",
        ),
        ("1. a\n   - x\n2. b|\n   c", "1. a\n   - x\n   - b|\n     c"),
        ("- a\n-  b|\n   c", "- a\n  -  b|\n     c"),
    ]);
}

#[test]
fn tab_off_a_list_touches_no_item() {
    for case in [
        "para|",
        "```\nx|\n```",
        "- a\n  ```\n  x|\n  ```",
        "- a\n\n|\n- b",
        "---\ntags:\n  - |\n---\nbody",
    ] {
        assert_eq!(run(indent_items, case), None, "{case:?}");
    }
}

#[test]
fn shift_tab_makes_an_item_its_parents_sibling() {
    shift_tab(&[
        ("- a\n  - |b", "- a\n- |b"),
        ("- a\n  |- b", "- a\n|- b"),
        ("- a\n  - b|", "- a\n- b|"),
        ("> - a\n>   - b|", "> - a\n> - b|"),
        ("- a\n  - b\n    - c|", "- a\n  - b\n  - c|"),
        ("- a\n  - b|\n    c\n    - d", "- a\n- b|\n  c\n  - d"),
        ("- a\n  - b|\nlazy", "- a\n- b|\nlazy"),
        ("- a\n  - b|\n   lazy", "- a\n- b|\n lazy"),
    ]);
}

/// The outdented marker takes its new list's kind: its parent's byte, or its number plus one.
#[test]
fn shift_tab_takes_the_parents_kind() {
    shift_tab(&[
        ("- a\n  * b|", "- a\n- b|"),
        ("1. a\n   - b|", "1. a\n2. b|"),
        ("1) a\n   - |b\n   - c|", "1) a\n2) b\n3) c"),
        ("09. a\n    - b|", "09. a\n10. b|"),
    ]);
}

/// The siblings below stay in place and become the outdented item's children; an ordered run of
/// them restarts at 1 while its numbers ran in sequence.
#[test]
fn shift_tab_adopts_the_siblings_below() {
    shift_tab(&[
        ("- a\n  - b|\n  - c", "- a\n- b|\n  - c"),
        ("- a\n  1. b|\n  2. c", "- a\n- b|\n  1. c"),
        ("- a\n  1. b|\n  2. c\n  3. d", "- a\n- b|\n  1. c\n  2. d"),
        ("- a\n  1. b|\n  2. c\n  7. d", "- a\n- b|\n  1. c\n  7. d"),
        ("- a\n  - b|\n\n  para", "- a\n- b|\n\n  para"),
    ]);
}

/// The rest of the parent's block moves to the new content column; a renumbered marker that
/// narrows shifts its own lines (D7).
#[test]
fn shift_tab_shifts_the_lines_the_new_marker_width_moves() {
    shift_tab(&[
        (
            "9. a\n   - b|\n     x\n   - c\n     y",
            "9. a\n10. b|\n    x\n    - c\n      y",
        ),
        (
            "- a\n\n  9. b|\n  10. c\n      y",
            "- a\n\n- b|\n  1. c\n     y",
        ),
        ("10. a\n    - b|\n      x", "10. a\n11. b|\n    x"),
        // `y` takes the rest's shift and its own narrowing, which cancel.
        (
            "9. a\n   1. b|\n   10. c\n       y",
            "9. a\n10. b|\n    1. c\n       y",
        ),
    ]);
}

#[test]
fn shift_tab_cuts_inside_a_tab_by_spaces() {
    shift_tab(&[
        ("- a\n\t- b|", "- a\n- b|"),
        ("- a\n  - b\n\t  - c|", "- a\n  - b\n  - c|"),
        (">\t- a\n>\t  - b|", ">\t- a\n>\t- b|"),
        ("> - a\n>\t- b|", "> - a\n> - b|"),
    ]);
    // A lazy line with fewer blanks than the cut loses all it has, its quote kept.
    shift_tab(&[("> - a\n>   - b|\n>  lazy", "> - a\n> - b|\n> lazy")]);
}

#[test]
fn shift_tab_on_a_top_level_item_writes_nothing() {
    for case in ["- a|", "1. a\n2. b|", "> - a|", "- - a|"] {
        assert_eq!(run(outdent_items, case), Some(case.to_owned()), "{case:?}");
    }
}

/// A line in an item's code gives no item: the item does not move.
#[test]
fn shift_tab_inside_an_items_code_touches_no_item() {
    let case = "- a\n  - b\n    ```\n    x|\n    ```";
    assert_eq!(run(outdent_items, case), None);
}

/// A marker the parse reads as no item still moves under `⇧⇥`, and stays under `⇥` (D8).
#[test]
fn an_empty_marker_the_parse_reads_as_no_item() {
    shift_tab(&[
        ("- p\n  - |", "- p\n- |"),
        ("- p\n  * |", "- p\n- |"),
        ("1. a\n   1. |", "1. a\n2. |"),
    ]);
    for case in ["- p\n  - |", "- p\n  * |", "1. a\n   1. |", "para\n- |"] {
        assert_eq!(run(indent_items, case), Some(case.to_owned()), "{case:?}");
    }
    assert_eq!(
        run(outdent_items, "para\n- |"),
        Some("para\n- |".to_owned())
    );
}

/// No command parses the note; the fallback reads each touched line once at most.
#[test]
fn tab_and_shift_tab_run_one_fallback_pass_per_line_at_most() {
    for case in ["- p\n  - |", "- a\n- b|", "para|"] {
        for command in [indent_items as Command, outdent_items] {
            FALLBACKS.set(0);
            run(command, case);
            assert!(FALLBACKS.get() <= 1, "{case:?}");
        }
    }
}

/// A child list of the parent right below the run can no longer interrupt a paragraph past 1.
#[test]
fn shift_tab_restarts_a_child_list_right_below_at_one() {
    shift_tab(&[
        ("- a\n  - b|\n  9. c", "- a\n- b|\n  1. c"),
        ("- a\n  - b|\n\n  9. c", "- a\n- b|\n\n  9. c"),
    ]);
}

/// Under a child list that ends the outdented item's block, the adopted siblings join it.
#[test]
fn shift_tab_joins_the_adopted_siblings_to_the_items_children() {
    shift_tab(&[
        ("- a\n  - b|\n    * x\n  - c", "- a\n- b|\n  * x\n  * c"),
        (
            "- a\n  - b|\n    1. x\n  - c\n  - d",
            "- a\n- b|\n  1. x\n  2. c\n  3. d",
        ),
        ("- a\n  - b|\n    1) x\n  1) c", "- a\n- b|\n  1) x\n  2) c"),
    ]);
    // A paragraph between keeps them a list of their own.
    shift_tab(&[(
        "- a\n  - b|\n    1. x\n\n  p\n  1. c",
        "- a\n- b|\n  1. x\n\n  p\n  1. c",
    )]);
}

/// A line already at the column it would move to takes no hunk.
#[test]
fn shift_tab_writes_no_hunk_that_changes_nothing() {
    let text = "- a\n  - b\nlazy";
    let edit = outdent_items(text, &parse(text), at(9)..at(9)).expect("an item");
    assert!(
        edit.hunks
            .iter()
            .all(|hunk| text[hunk.range.start.get()..hunk.range.end.get()] != hunk.text),
        "{:?}",
        edit.hunks
    );
}
