//! Properties of `⇥` and `⇧⇥` at a caret over generated notes, checked against the pulldown-cmark
//! oracle (list editing spec, Tests: P0, P2, P3).

use std::ops::Range;

use inkra_text::ByteOffset;
use proptest::prelude::*;

use super::{indent_items, outdent_items};
use crate::list_edit::notes;
use crate::list_edit::oracle::{
    Read, caret_item, empty_above_blank, indented, is_empty_item, oracle, outdented, parent,
    previous_sibling, quotes_kept, shape,
};
use crate::spans::{line_end, line_start};
use crate::testing::{apply, edited};
use crate::{ListEdit, parse};

fn caret(at: usize) -> Range<ByteOffset> {
    ByteOffset::new(at)..ByteOffset::new(at)
}

/// A command's edit, the oracle's items, and the caret item among them.
type Touched = (ListEdit, Vec<Read>, Option<usize>);

/// The command's edit and the caret item the oracle reads; `None` when the command touches no
/// item.
fn touched(
    command: fn(&str, &crate::Markdown, Range<ByteOffset>) -> Option<ListEdit>,
    text: &str,
    at: usize,
) -> Result<Option<Touched>, TestCaseError> {
    // A blank line's holder is D1's, which the oracle does not restate.
    let line = line_start(text, at);
    prop_assume!(
        !text[line..line_end(text, line)]
            .trim_matches([' ', '\t', '>'])
            .is_empty()
    );
    let Some(edit) = command(text, &parse(text), caret(at)) else {
        return Ok(None);
    };
    let (items, _) = oracle(text);
    let item = caret_item(text, &items, line);
    Ok(Some((edit, items, item)))
}

/// P0: the items keep their depth, list and order but those the command moves, and every quote
/// still opens.
fn structure(text: &str, edit: &ListEdit, want: Option<Vec<Read>>) -> Result<(), TestCaseError> {
    let Some(want) = want else {
        prop_assert!(edit.hunks.is_empty(), "{:?} gave {:?}", text, edit.hunks);
        return Ok(());
    };
    let new = apply(text, &edit.hunks);
    prop_assert_eq!(
        shape(&oracle(&new).0),
        shape(&want),
        "{:?} gave {:?}",
        text,
        new
    );
    quotes_kept(text, edit, &new).map_err(TestCaseError::fail)
}

/// `⇥` makes the caret item a child of its previous sibling, with its descendants.
fn indents(text: &str, at: usize) -> Result<(), TestCaseError> {
    let Some((edit, items, item)) = touched(indent_items, text, at)? else {
        return Ok(());
    };
    let Some(item) = item else {
        prop_assert!(
            edit.hunks.is_empty(),
            "{:?} at {} gave {:?}",
            text,
            at,
            edit.hunks
        );
        return Ok(());
    };
    let want = indented(text, &items, item);
    // An empty item above a blank line cannot hold the run (Known limits).
    let above = previous_sibling(&items, item);
    prop_assume!(!above.is_some_and(|above| empty_above_blank(text, &items, above)));
    // A first child cannot interrupt its parent's paragraph while empty (Risks).
    let joins = want
        .as_ref()
        .is_some_and(|want| items.iter().any(|old| old.list == want[item].list));
    prop_assume!(joins || !is_empty_item(text, &items, item));
    structure(text, &edit, want)
}

/// `⇧⇥` makes the caret item a sibling of its parent, and the siblings below it its children.
fn outdents(text: &str, at: usize) -> Result<(), TestCaseError> {
    let Some((edit, items, item)) = touched(outdent_items, text, at)? else {
        return Ok(());
    };
    // P3: with no item, nothing.
    let Some(item) = item else {
        prop_assert!(
            edit.hunks.is_empty(),
            "{:?} at {} gave {:?}",
            text,
            at,
            edit.hunks
        );
        return Ok(());
    };
    // P3: no non-blank byte outside the markers it rewrites.
    let blank = |text: &str| text.bytes().all(|byte| matches!(byte, b' ' | b'\t'));
    let marker = |text: &str| {
        let digits = text.trim_end_matches(['.', ')']);
        matches!(text, "-" | "*" | "+")
            || (digits.len() + 1 == text.len() && digits.bytes().all(|byte| byte.is_ascii_digit()))
    };
    for hunk in &edit.hunks {
        let range = hunk.range.start.get()..hunk.range.end.get();
        let removed = &text[range.clone()];
        let rewrites = marker(removed)
            && marker(&hunk.text)
            && items.iter().any(|old| old.marker == range.start);
        prop_assert!(
            removed != hunk.text,
            "{:?} at {} wrote {:?}",
            text,
            at,
            hunk
        );
        prop_assert!(
            (blank(removed) && blank(&hunk.text)) || rewrites,
            "{:?} at {} wrote {:?}",
            text,
            at,
            hunk
        );
    }
    let shared = parent(&items, item).is_some_and(|parent| {
        line_start(text, items[parent].marker) == line_start(text, items[item].marker)
    });
    let want = outdented(text, &items, item).filter(|_| !shared);
    // An empty item above a blank line cannot hold the siblings it adopts (Known limits); an empty
    // first child cannot interrupt a paragraph (Risks).
    prop_assume!(!empty_above_blank(text, &items, item));
    let below = (item + 1..items.len()).find(|&next| items[next].depth <= items[item].depth);
    prop_assume!(!below.is_some_and(|below| is_empty_item(text, &items, below)));
    structure(text, &edit, want)
}

/// P2: on a bullet list, `⇥` then `⇧⇥` gives the text back.
fn round_trip(text: &str, at: usize) -> Result<(), TestCaseError> {
    let (items, _) = oracle(text);
    // A blank line's holder is D1's, which moves with the edit.
    let line = line_start(text, at);
    if text[line..line_end(text, line)]
        .trim_matches([' ', '\t', '>'])
        .is_empty()
    {
        return Ok(());
    }
    let Some(edit) =
        indent_items(text, &parse(text), caret(at)).filter(|edit| !edit.hunks.is_empty())
    else {
        return Ok(());
    };
    // An empty first child reads as no item until typed (Risks), so `⇧⇥` leaves its block behind;
    // an empty item above a blank line holds no child (Known limits).
    let item = caret_item(text, &items, line_start(text, at));
    prop_assume!(!item.is_some_and(|item| is_empty_item(text, &items, item)));
    let above = item.and_then(|item| previous_sibling(&items, item));
    prop_assume!(!above.is_some_and(|above| empty_above_blank(text, &items, above)));
    let (indented, landed) = edited(text, &edit);
    let back = outdent_items(&indented, &parse(&indented), caret(landed));
    let back = back.map(|back| apply(&indented, &back.hunks));
    prop_assert_eq!(
        back.as_deref(),
        Some(text),
        "{:?} at {} gave {:?}",
        text,
        at,
        indented
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_global_rejects: 4096,
        max_local_rejects: 4096,
        ..ProptestConfig::with_cases(2000)
    })]

    #[test]
    fn tab_keeps_the_structure_it_means((text, at) in notes::case()) {
        indents(&text, at)?;
    }

    #[test]
    fn shift_tab_keeps_the_structure_it_means((text, at) in notes::case()) {
        outdents(&text, at)?;
    }

    #[test]
    fn tab_then_shift_tab_gives_a_bullet_list_back((text, at) in notes::bullet_case()) {
        round_trip(&text, at)?;
    }
}
