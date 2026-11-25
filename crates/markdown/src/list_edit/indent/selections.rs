//! P0 for `⇥` and `⇧⇥` on a selection over several lines: each touched run moves as one caret on
//! it would, the later runs first under `⇧⇥` (list editing spec, D4, Tests: P0).

use std::iter;

use inkra_text::ByteOffset;
use proptest::prelude::*;
use proptest::sample::Index;

use super::{indent_items, outdent_items};
use crate::list_edit::notes;
use crate::list_edit::oracle::{
    FRESH, Read, caret_item, empty_above_blank, indented, is_empty_item, oracle, outdented, parent,
    previous_sibling, quotes_kept, shape, subtree,
};
use crate::parse;
use crate::spans::{line_end, line_start};
use crate::testing::apply;

/// The start of each line a selection from `start` to `end` touches: the first, then each that
/// starts before `end`.
fn lines(text: &str, start: usize, end: usize) -> Vec<usize> {
    let first = line_start(text, start);
    iter::once(first)
        .chain(
            text.match_indices('\n')
                .map(|(at, _)| at + 1)
                .filter(|&line| line > first && line < end),
        )
        .collect()
}

/// The start of each line a fenced code block touches, its fences included.
fn code_lines(text: &str) -> Vec<usize> {
    let mut open = false;
    lines(text, 0, text.len())
        .into_iter()
        .filter(|&line| {
            let fence = text[line..line_end(text, line)]
                .trim_start_matches(['>', ' ', '\t'])
                .starts_with("```");
            let code = open || fence;
            open ^= fence;
            code
        })
        .collect()
}

/// The items the lines give, by `⌘L`'s rule: each line's caret item, none for a blank line or a
/// code line; an item moves with a touched ancestor only. In document order.
fn touched(text: &str, items: &[Read], lines: &[usize]) -> Vec<usize> {
    let code = code_lines(text);
    let mut out: Vec<usize> = lines
        .iter()
        .filter(|&&line| {
            let bytes = text[line..line_end(text, line)].bytes();
            !code.contains(&line) && !bytes.clone().all(|byte| matches!(byte, b' ' | b'\t'))
        })
        .filter_map(|&line| caret_item(text, items, line))
        .collect();
    out.sort_unstable();
    out.dedup();
    let given = out.clone();
    out.retain(|&at| {
        !iter::successors(parent(items, at), |&up| parent(items, up)).any(|up| given.contains(&up))
    });
    out
}

/// `touched` in runs of consecutive siblings.
fn runs(items: &[Read], touched: &[usize]) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for &at in touched {
        match runs.last_mut() {
            Some(run)
                if subtree(items, run[run.len() - 1]).end == at
                    && items[at].list == items[run[0]].list =>
            {
                run.push(at);
            }
            _ => runs.push(vec![at]),
        }
    }
    runs
}

/// Gives each list `step` started a name of its own, so two new lists stay apart.
fn named(want: &mut [Read], step: usize) {
    for item in want.iter_mut().filter(|item| item.list == FRESH) {
        item.list = FRESH - 1 - step;
    }
}

/// The items the command means: `⇥` on each run in order, `⇧⇥` from the last run's last item up,
/// so an item outdented later in the source no longer reads as a sibling to adopt.
fn meant(text: &str, items: &[Read], runs: &[Vec<usize>], indent: bool) -> Vec<Read> {
    let mut want = items.to_vec();
    let mut step = 0;
    let mut moves: Vec<usize> = Vec::new();
    for run in runs {
        let shared = parent(items, run[0]).is_some_and(|parent| {
            line_start(text, items[parent].marker) == line_start(text, items[run[0]].marker)
        });
        let movable = if indent {
            previous_sibling(items, run[0]).is_some()
        } else {
            !shared
        };
        if movable {
            moves.extend(run);
        }
    }
    if !indent {
        moves.reverse();
    }
    for at in moves {
        let next = if indent {
            indented(text, &want, at)
        } else {
            outdented(text, &want, at)
        };
        if let Some(next) = next {
            want = next;
        }
        named(&mut want, step);
        step += 1;
    }
    want
}

fn check(indent: bool, text: &str, at: usize, end: Index) -> Result<(), TestCaseError> {
    let end = at + end.index(text.len() - at + 1);
    let lines = lines(text, at, end);
    prop_assume!(lines.len() > 1);
    // A quoted blank line's holder is D1's, which the oracle does not restate.
    prop_assume!(!lines.iter().any(|&line| {
        let line = &text[line..line_end(text, line)];
        line.contains('>') && line.trim_matches([' ', '\t', '>']).is_empty()
    }));
    let (items, _) = oracle(text);
    // An empty item holds nothing past a blank line, yet `caret_item` gives it the lines there.
    prop_assume!(!(0..items.len()).any(|at| empty_above_blank(text, &items, at)));
    let touched = touched(text, &items, &lines);
    let selection = ByteOffset::new(at)..ByteOffset::new(end);
    let command = if indent { indent_items } else { outdent_items };
    let Some(edit) = command(text, &parse(text), selection) else {
        prop_assert!(touched.is_empty(), "{:?} from {} to {}", text, at, end);
        return Ok(());
    };
    // An empty item reads as no item until typed (Risks).
    let empty = |at: usize| is_empty_item(text, &items, at);
    prop_assume!(!touched.iter().any(|&at| {
        let below = subtree(&items, at).end;
        empty(at)
            || previous_sibling(&items, at).is_some_and(empty)
            || (below < items.len() && empty(below))
    }));
    let want = meant(text, &items, &runs(&items, &touched), indent);
    let new = apply(text, &edit.hunks);
    prop_assert_eq!(
        shape(&oracle(&new).0),
        shape(&want),
        "{:?} from {} to {} gave {:?}",
        text,
        at,
        end,
        new
    );
    quotes_kept(text, &edit, &new).map_err(TestCaseError::fail)
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_global_rejects: 16384,
        max_local_rejects: 16384,
        ..ProptestConfig::with_cases(2000)
    })]

    #[test]
    fn tab_on_a_selection_keeps_the_structure_it_means((text, at) in notes::case(), end: Index) {
        check(true, &text, at, end)?;
    }

    #[test]
    fn shift_tab_on_a_selection_keeps_the_structure_it_means(
        (text, at) in notes::case(),
        end: Index,
    ) {
        check(false, &text, at, end)?;
    }
}
