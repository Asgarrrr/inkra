//! Properties of `↩` over generated notes, checked against a pulldown-cmark oracle of their own
//! (list editing spec, Tests: P0, P1's continue half, P4, P5).

mod skips;

use inkra_text::{ByteOffset, Hunk};
use proptest::prelude::*;

use super::continue_list;
use crate::list_edit::notes;
use crate::list_edit::oracle::{
    containers, empty_above_blank, oracle, outdented, quotes_kept, shape,
};
use crate::spans::{line_end, line_start};
use crate::testing::{edited, landing};
use crate::{ListEdit, Markdown, parse};
use skips::{blank_under_empty_marker, splits_opener, unheld_quoted_code};

/// Each `\n` the hunks insert, as an offset in the edited text.
fn inserted_newlines(hunks: &[Hunk]) -> Vec<usize> {
    let mut grown = 0isize;
    let mut out = Vec::new();
    for hunk in hunks {
        let start = hunk.range.start.get().saturating_add_signed(grown);
        out.extend(hunk.text.match_indices('\n').map(|(at, _)| start + at));
        grown += hunk.text.len().cast_signed() - (hunk.range.end - hunk.range.start).cast_signed();
    }
    out
}

/// The caret's line is a marker line, the caret at its end right past text (D2, rule 6).
fn past_text_at_end(text: &str, markdown: &Markdown, caret: usize) -> bool {
    let line = line_start(text, caret);
    let Some(gutter) = markdown.gutter_on(ByteOffset::new(line)) else {
        return false;
    };
    let typed = &text[gutter.range.end.get().min(caret)..caret];
    gutter.item.as_ref().is_some_and(|item| item.marked)
        && caret == line_end(text, line)
        && !typed.trim().is_empty()
        && !typed.ends_with([' ', '\t'])
}

/// P0: the items keep their depth, list and order, but the one the command adds or removes, and
/// every quote still opens.
fn intended_structure(
    text: &str,
    caret: usize,
    edit: &ListEdit,
    new: &str,
) -> Result<(), TestCaseError> {
    let (old_items, _) = oracle(text);
    let (new_items, _) = oracle(new);
    let line = line_start(text, caret);
    let (old_shape, new_shape) = (shape(&old_items), shape(&new_items));
    let fail = || TestCaseError::fail(format!("{text:?} at {caret} gave {new:?}"));
    let index = old_items
        .iter()
        .rposition(|item| line_start(text, item.marker) == line);
    // A nested empty item moves up one level, as `⇧⇥` moves it (D3).
    let outdents = index
        .filter(|&index| old_items[index].depth > 0)
        .filter(|_| !edit.hunks.iter().any(|hunk| hunk.text.contains('\n')));
    prop_assume!(!outdents.is_some_and(|index| empty_above_blank(text, &old_items, index)));
    let want = if new_shape.len() == old_shape.len() {
        outdents
            .and_then(|index| outdented(text, &old_items, index))
            .map_or(old_shape, |want| shape(&want))
    } else {
        let index = index.ok_or_else(fail)?;
        let mut want = old_items.clone();
        if new_shape.len() > old_shape.len() {
            want.insert(index, old_items[index]);
        } else {
            // A list of one item gone may join the lists around it, which P0 does not state
            // (open question, reported).
            let list = old_items[index].list;
            prop_assume!(old_items.iter().filter(|item| item.list == list).count() > 1);
            want.remove(index);
        }
        shape(&want)
    };
    prop_assert_eq!(&new_shape, &want, "{:?} at {} gave {:?}", text, caret, new);

    quotes_kept(text, edit, new).map_err(|error| TestCaseError::fail(format!("{error} at {caret}")))
}

/// P4 and P1's continue half: the new line holds a gutter item, a task when the caret item was
/// one; without it, the text and the caret come back, up to the blanks and `>` before a line the
/// command shifted or gave a prefix.
fn continued(
    text: &str,
    markdown: &Markdown,
    caret: usize,
    edit: &ListEdit,
    (new, at): (&str, usize),
) -> Result<(), TestCaseError> {
    let parsed = parse(new);
    let new_line = ByteOffset::new(line_start(new, at));
    let item = parsed
        .gutter_on(new_line)
        .and_then(|gutter| gutter.item.as_ref());
    prop_assert!(
        item.is_some_and(|item| item.marked),
        "{:?} at {} gave {:?}",
        text,
        caret,
        new
    );
    let task = markdown
        .task_at(ByteOffset::new(line_start(text, caret)))
        .is_some();
    prop_assert_eq!(
        parsed.task_at(new_line).is_some(),
        task,
        "{:?} at {} gave {:?}",
        text,
        caret,
        new
    );

    let from = new_line.get() - 1;
    let restored = format!("{}{}", &new[..from], &new[at..]);
    let bare = |text: &str| {
        text.lines()
            .map(|line| line.trim_start_matches([' ', '\t', '>']).to_owned())
            .collect::<Vec<_>>()
    };
    if edit.hunks.len() == 1 {
        prop_assert_eq!(&restored, text, "{:?} at {} gave {:?}", text, caret, new);
    } else {
        prop_assert_eq!(
            bare(&restored),
            bare(text),
            "{:?} at {} gave {:?}",
            text,
            caret,
            new
        );
    }
    // Past blank lines, the new line goes below them and the caret with it.
    let past_blanks = edit.hunks[0].range.start.get() > caret;
    prop_assert!(
        from == caret || past_blanks,
        "{:?} at {} gave {:?}",
        text,
        caret,
        new
    );
    Ok(())
}

fn check(text: &str, caret: usize) -> Result<(), TestCaseError> {
    prop_assume!(!splits_opener(text, caret));
    let markdown = parse(text);
    prop_assume!(!unheld_quoted_code(text, &markdown, caret));
    let Some(edit) = continue_list(text, &markdown, ByteOffset::new(caret)) else {
        return Ok(());
    };
    // Rule 8's exit ends the quote on purpose, so what follows goes in a new one.
    let exits = matches!(&edit.hunks[..], [hunk]
        if hunk.text.is_empty() && text.as_bytes()[hunk.range.start.get()] == b'>');
    prop_assume!(!exits || line_end(text, caret) == text.len());
    // Disjoint and in order, or `edited` panics.
    let (new, landed) = edited(text, &edit);
    prop_assume!(!blank_under_empty_marker(text, &markdown, caret, &new));
    prop_assert!(landed <= new.len(), "caret {} past {:?}", landed, new);

    intended_structure(text, caret, &edit, &new)?;
    // P0: when `↩` adds a line, every other line keeps its quotes and the count of items holding
    // it. D3's deletion may let a line join the item above, by design.
    let caret_line = line_start(text, caret);
    let adds = edit.hunks.iter().any(|hunk| hunk.text.contains('\n'));
    let mut start = if adds { 0 } else { text.len() };
    while start < text.len() {
        let end = line_end(text, start);
        let touched = edit.hunks.iter().any(|hunk| {
            let range = hunk.range.start.get()..hunk.range.end.get();
            range.contains(&start) && range.start < start
        });
        if start != caret_line
            && !text[start..end].trim_matches([' ', '\t', '>']).is_empty()
            && !touched
        {
            let to = line_start(&new, landing(&edit, start));
            prop_assert_eq!(
                containers(&new, to),
                containers(text, start),
                "line {} of {:?} at {} gave {:?}",
                start,
                text,
                caret,
                new
            );
        }
        start = end + 1;
    }
    // P5: each inserted `\n` starts a line in the caret line's quotes and items.
    let held = containers(text, line_start(text, caret));
    for newline in inserted_newlines(&edit.hunks) {
        prop_assert_eq!(
            containers(&new, newline + 1),
            held,
            "{:?} at {} gave {:?}",
            text,
            caret,
            new
        );
    }
    if past_text_at_end(text, &markdown, caret) {
        continued(text, &markdown, caret, &edit, (&new, landed))?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_global_rejects: 4096,
        max_local_rejects: 4096,
        ..ProptestConfig::with_cases(2000)
    })]

    #[test]
    fn enter_keeps_the_structure_it_means((text, caret) in notes::case()) {
        check(&text, caret)?;
    }
}
