//! `↩` per rule, each case written `text` with `|` for the caret (list editing spec, D2, D3, D8).

use inkra_text::{ByteOffset, Hunk};

use super::{FALLBACKS, continue_list};
use crate::parse;

pub(super) fn apply(text: &str, hunks: &[Hunk]) -> String {
    let mut out = text.to_owned();
    for hunk in hunks.iter().rev() {
        out.replace_range(hunk.range.start.get()..hunk.range.end.get(), &hunk.text);
    }
    out
}

/// `case`'s text after `↩` at its `|`, with `|` at the caret; `None` for the plain key.
fn enter(case: &str) -> Option<String> {
    let caret = case.find('|').expect("a caret");
    let text = case.replacen('|', "", 1);
    let edit = continue_list(&text, &parse(&text), ByteOffset::new(caret))?;
    let mut out = apply(&text, &edit.hunks);
    out.insert(edit.caret.get(), '|');
    Some(out)
}

fn check(cases: &[(&str, &str)]) {
    for &(case, want) in cases {
        assert_eq!(enter(case).as_deref(), Some(want), "{case:?}");
    }
}

#[test]
fn past_an_items_text_the_next_sibling_continues_it() {
    check(&[
        ("- a|", "- a\n- |"),
        ("* a|", "* a\n* |"),
        ("+ a|", "+ a\n+ |"),
        ("1. a|", "1. a\n2. |"),
        ("1) a|", "1) a\n2) |"),
        ("09. a|", "09. a\n10. |"),
        ("01. a|", "01. a\n02. |"),
        ("- - a|", "- - a\n  - |"),
        ("-   a|", "-   a\n-   |"),
        ("-\ta|", "-\ta\n-\t|"),
        ("- a\n  - b|", "- a\n  - b\n  - |"),
        ("- a\n\t- b|", "- a\n\t- b\n    - |"),
        ("  - a|", "  - a\n  - |"),
    ]);
}

#[test]
fn a_task_continues_as_an_unchecked_task() {
    check(&[
        ("- [ ] a|", "- [ ] a\n- [ ] |"),
        ("- [x] a|", "- [x] a\n- [ ] |"),
        ("* [X] a|", "* [X] a\n* [ ] |"),
        ("1. [x] a|", "1. [x] a\n2. [ ] |"),
        ("- [ ]  a|", "- [ ]  a\n- [ ] |"),
    ]);
}

/// The text after the caret goes to the new item; the blanks before it go.
#[test]
fn inside_an_items_text_the_item_splits() {
    check(&[
        ("- it|em", "- it\n- |em"),
        ("- a |b", "- a\n- |b"),
        ("- a|b\nc", "- a\n- |b\nc"),
        ("- it|em\n  - c", "- it\n- |em\n  - c"),
        ("1. [x] a|b", "1. [x] a\n2. [ ] |b"),
    ]);
}

#[test]
fn a_quoted_item_continues_in_its_quote() {
    check(&[
        ("> - a|", "> - a\n> - |"),
        ("> 1. [x] a|", "> 1. [x] a\n> 2. [ ] |"),
        ("> > - a|", "> > - a\n> > - |"),
        (">- a|", ">- a\n> - |"),
        ("  > - a|", "  > - a\n  > - |"),
        (">\t- a|", ">\t- a\n>   - |"),
    ]);
}

/// The lines the item holds below move with the new item, which takes its children (decision 6).
#[test]
fn the_new_item_takes_the_lines_below() {
    check(&[
        ("- a|\n  - c", "- a\n- |\n  - c"),
        ("- a|\n  b", "- a\n- |\n  b"),
        ("- a\n- |\n  - c", "- a\n- \n- |\n  - c"),
        ("- a\n-|\n  - c", "- a\n-\n- |\n  - c"),
        ("- a\n-      |\n  - c", "- a\n- \n- |\n  - c"),
        ("- a|\n  ```\n  x\n  ```", "- a\n- |\n  ```\n  x\n  ```"),
    ]);
}

/// A lazy line right below an empty new marker line would leave the list, so it takes the new
/// item's container prefix.
#[test]
fn a_lazy_line_below_an_empty_marker_line_takes_the_container_prefix() {
    check(&[
        ("- a|\nb", "- a\n- |\n  b"),
        ("> - a|\nb", "> - a\n> - |\n>   b"),
        ("-   a|\n  b", "-   a\n-   |\n    b"),
        ("- a|\nb\nc", "- a\n- |\n  b\nc"),
        ("> - a|\n  b", "> - a\n> - |\n>   b"),
    ]);
}

#[test]
fn past_blank_lines_the_new_item_keeps_the_lines_below_in_the_list() {
    check(&[
        ("- a|\n\n  b", "- a\n\n- |\n  b"),
        ("- a|\n\n\n  b", "- a\n\n\n- |\n  b"),
        ("> - a|\n>\n>   b", "> - a\n>\n> - |\n>   b"),
    ]);
    // Nothing the item holds follows the blank lines: the new item goes right below.
    check(&[
        ("- a|\n\n- b", "- a\n- |\n\n- b"),
        ("- a|\n\npara", "- a\n- |\n\npara"),
    ]);
}

/// A wider marker moves its content column, so the moved lines shift with it (D7).
#[test]
fn a_wider_marker_shifts_the_lines_it_takes() {
    check(&[
        ("9. a|\n   - c", "9. a\n10. |\n    - c"),
        ("9. a|\n   b\n\n   c", "9. a\n10. |\n    b\n\n    c"),
        ("9. a|\nb", "9. a\n10. |\n    b"),
        ("9. a|b\n   c", "9. a\n10. |b\n    c"),
        ("9. a|\n\n   b", "9. a\n\n10. |\n    b"),
        ("9. a|\n\t- c", "9. a\n10. |\n\t - c"),
        ("> 9. a|\n>    - c", "> 9. a\n> 10. |\n>     - c"),
        (
            "9. a|\n   ```\n   x\n   ```",
            "9. a\n10. |\n    ```\n    x\n    ```",
        ),
        ("- b| c\n  - a", "- b\n-  |c\n   - a"),
        ("- b|\tc\n  - a", "- b\n- \t|c\n    - a"),
        ("- b|      c\n  - a", "- b\n- |      c\n  - a"),
    ]);
}

/// Each `↩` is one hunk unless it moves lines that change, so `⌘Z` puts the caret back (D9).
#[test]
fn a_continuation_with_nothing_to_shift_is_one_hunk() {
    for case in ["- a|\n  - c", "- a|\n\n  b", "1. a|\n   b"] {
        let caret = case.find('|').expect("a caret");
        let text = case.replacen('|', "", 1);
        let edit = continue_list(&text, &parse(&text), ByteOffset::new(caret));
        assert_eq!(edit.map(|edit| edit.hunks.len()), Some(1), "{case:?}");
    }
}

/// An empty item above the text could not interrupt a paragraph, so a first item gets a blank
/// line above instead.
#[test]
fn at_an_items_text_start_a_sibling_or_a_blank_line_goes_above() {
    check(&[
        ("- a\n- |b", "- a\n- \n- |b"),
        ("1. a\n2. |b", "1. a\n2. \n2. |b"),
        ("- [x] a\n- [x] |b", "- [x] a\n- [ ] \n- [x] |b"),
        ("> - a\n> - |b", "> - a\n> - \n> - |b"),
        ("- a\n  - b\n  - |c", "- a\n  - b\n  - \n  - |c"),
        ("- a\n  - b\n- |c", "- a\n  - b\n- \n- |c"),
        ("para\n- |abc", "para\n\n- |abc"),
        ("- p\n  - |abc", "- p\n\n  - |abc"),
        ("> para\n> - |abc", "> para\n>\n> - |abc"),
        ("- |a", "\n- |a"),
        ("-   |a", "\n-   |a"),
        ("1. a\n\npara\n\n2. |x", "1. a\n\npara\n\n\n2. |x"),
    ]);
}

/// Until L2, a nested empty item leaves the list as a top-level one does.
#[test]
fn an_empty_item_leaves_the_list() {
    check(&[
        ("- a\n- |", "- a\n|"),
        ("- |", "|"),
        ("- [ ] |", "|"),
        ("1. a\n2. |", "1. a\n|"),
        ("-   |", "|"),
        ("- |  ", "|"),
        ("> - |", "> |"),
        ("> - a\n> - |", "> - a\n> |"),
        ("- a\n  - b\n  - |", "- a\n  - b\n|"),
        ("1. a\n   - b\n   - |", "1. a\n   - b\n|"),
        ("- a\n  - b\n    - c\n    - |", "- a\n  - b\n    - c\n|"),
        ("- a\n- |\n\npara", "- a\n|\n\npara"),
    ]);
}

/// A marker the parse reads as a setext underline or as text is still an empty item (D8).
#[test]
fn an_empty_marker_the_parse_reads_as_no_item_leaves_the_list() {
    check(&[
        ("- p\n  - |", "- p\n|"),
        ("- p\n  * |", "- p\n|"),
        ("1. a\n   1. |", "1. a\n|"),
        ("para\n- |", "para\n|"),
        ("para\n-|", "para\n|"),
        ("> para\n> - |", "> para\n> |"),
    ]);
    assert_eq!(enter("- p\n  |- "), None, "before its text start");
    assert_eq!(enter("para\n2. b|"), None, "a lazy number holds text");
}

#[test]
fn a_held_line_goes_on_in_its_item() {
    check(&[
        ("- a\n  b|", "- a\n  b\n  |"),
        ("- a\n  b |c", "- a\n  b\n  |c"),
        ("- a\n  b|\n  c", "- a\n  b\n  |\n  c"),
        ("- a\n  - b\n    c|", "- a\n  - b\n    c\n    |"),
        ("- a\n  - b\n\n  c|", "- a\n  - b\n\n  c\n  |"),
        ("-   a\n    b|", "-   a\n    b\n    |"),
        ("- a\nb|", "- a\nb\n  |"),
        ("> - a\nb|", "> - a\nb\n>   |"),
        ("- [ ] a\n  b|", "- [ ] a\n  b\n  |"),
    ]);
}

/// The line the caret leaves turns blank, so it keeps its quote part alone, trimmed.
#[test]
fn at_a_held_lines_text_start_the_line_left_keeps_its_quote_part() {
    check(&[
        ("- a\n  |b", "- a\n\n  |b"),
        ("> - a\n>   |b", "> - a\n>\n>   |b"),
        ("> > - a\n> >   |b", "> > - a\n> >\n> >   |b"),
    ]);
}

#[test]
fn a_code_line_goes_on_in_its_block() {
    check(&[
        ("- [ ] a\n  ```\n  x|", "- [ ] a\n  ```\n  x\n  |"),
        ("- a\n\n      x|", "- a\n\n      x\n      |"),
        ("- a\n  ```\n|\n  x", "- a\n  ```\n\n  |\n  x"),
        ("> ```\n> x|", "> ```\n> x\n> |"),
        ("> ```\n> x\n> |", "> ```\n> x\n>\n> |"),
        ("> - a\n>   ```\n>   x|", "> - a\n>   ```\n>   x\n>   |"),
    ]);
}

#[test]
fn a_fence_on_a_marker_line_opens_a_code_line_in_its_item() {
    check(&[
        ("- ```rust|", "- ```rust\n  |"),
        ("> - ```|", "> - ```\n>   |"),
        ("10. ```|", "10. ```\n    |"),
    ]);
}

#[test]
fn a_code_block_outside_every_item_and_quote_takes_the_plain_key() {
    for case in ["```\nx|", "```\n|", "    x|", "```\n- |"] {
        assert_eq!(enter(case), None, "{case:?}");
    }
}

#[test]
fn a_quote_line_continues_its_quote() {
    check(&[
        ("> a|", "> a\n> |"),
        ("> > a|", "> > a\n> > |"),
        ("> a |b", "> a\n> |b"),
        ("> |a", ">\n> |a"),
        (">a|", ">a\n> |"),
        ("  > a|", "  > a\n  > |"),
        ("> a\nb|", "> a\nb\n> |"),
        ("> > a\nb|", "> > a\nb\n> > |"),
    ]);
}

#[test]
fn an_empty_quote_line_leaves_its_innermost_quote() {
    check(&[
        ("> |", "|"),
        (">|", "|"),
        ("> > |", "> |"),
        ("> a\n> |", "> a\n|"),
        ("> a\n>   |", "> a\n|"),
    ]);
}

#[test]
fn before_a_text_start_the_plain_key_goes_in() {
    for case in ["|- a", "> |- a", "|> a", "- a\n|  b", "|"] {
        assert_eq!(enter(case), None, "{case:?}");
    }
}

/// An item that is no gutter item lies past the outer item's text start.
#[test]
fn a_line_holding_an_opener_past_its_text_start_takes_the_plain_key() {
    for case in ["- > - a|", "- > a|", "- a\n\n  > b|"] {
        assert_eq!(enter(case), None, "{case:?}");
    }
}

#[test]
fn a_plain_line_takes_the_plain_key() {
    for case in ["a|", "a\nb|", "# a|", "a\n\n|"] {
        assert_eq!(enter(case), None, "{case:?}");
    }
}

/// No `↩` parses the note; the fallback reads the caret's line once at most.
#[test]
fn enter_runs_one_fallback_pass_at_most() {
    for case in ["para\n- |", "- a|", "> a|", "a|", "- a\n  b|", "```\nx|"] {
        FALLBACKS.set(0);
        enter(case);
        assert!(FALLBACKS.get() <= 1, "{case:?}");
    }
}
