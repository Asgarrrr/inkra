use super::Candidate;
use super::fold::{PathChar, QueryChar};
use super::query::NameQuery;

const BOUNDARY: u8 = 24;
const STEM_START: u8 = 32;

/// A match here gains this, on top of the base score: a word start, a case change, the stem.
pub(super) fn bonus(raw: &[char], at: usize, stem: usize) -> u8 {
    let boundary = at == 0 || {
        let (prev, cur) = (raw[at - 1], raw[at]);
        matches!(prev, '/' | '-' | '_' | ' ' | '.') || (prev.is_lowercase() && cur.is_uppercase())
    };
    u8::from(boundary) * BOUNDARY + u8::from(at == stem) * STEM_START
}

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 16;
/// A gap of `g` chars costs `GAP_OPEN + g - 1`, at most `GAP_MAX`.
const GAP_OPEN: i32 = 3;
const GAP_MAX: i32 = 12;
const IN_STEM: i32 = 8;
const EXACT_STEM: i32 = 1024;
/// Far enough below any score that a few penalties cannot reach a real one.
const NONE: i32 = i32::MIN / 2;

/// `None` when some term is not a subsequence of the path.
pub(super) fn score(note: &Candidate, query: &NameQuery, table: &mut Vec<i32>) -> Option<i32> {
    if !note.bag.contains_all(query.bag) || !query.terms.iter().all(|term| fits(&note.chars, term))
    {
        return None;
    }
    let mut total = 0;
    for term in &query.terms {
        total += best(note, term, table)?.0;
    }
    if is_stem(note, &query.joined) {
        total += EXACT_STEM;
    }
    if query.showing == Some(note.id) {
        total -= total / 4;
    }
    Some(total)
}

/// Byte offsets into `note.path` of the chars the best alignments matched, ascending.
pub(super) fn positions(note: &Candidate, query: &NameQuery, table: &mut Vec<i32>) -> Vec<usize> {
    let mut at = Vec::new();
    for term in &query.terms {
        let Some((_, from)) = best(note, term, table) else {
            continue;
        };
        fill(note, term, from, table);
        backtrack(note, term, from, table, &mut at);
    }
    at.sort_unstable();
    at.dedup();
    let offsets: Vec<usize> = note.path.char_indices().map(|(offset, _)| offset).collect();
    at.into_iter().map(|index| offsets[index]).collect()
}

/// Cheap reject before the DP: greedy subsequence.
fn fits(chars: &[PathChar], term: &[QueryChar]) -> bool {
    let mut chars = chars.iter();
    term.iter().all(|q| chars.any(|&c| q.matches(c)))
}

fn is_stem(note: &Candidate, joined: &[QueryChar]) -> bool {
    let stem = &note.chars[note.stem..];
    stem.len() == joined.len() && joined.iter().zip(stem).all(|(q, &c)| q.matches(c))
}

/// The term's best score and the char it was aligned from: the whole path, or the stem alone
/// with its bonus.
fn best(note: &Candidate, term: &[QueryChar], table: &mut Vec<i32>) -> Option<(i32, usize)> {
    let whole = fill(note, term, 0, table)?;
    let length = i32::try_from(term.len()).unwrap_or(i32::MAX);
    let in_stem = if fits(&note.chars[note.stem..], term) {
        fill(note, term, note.stem, table).map(|s| s + IN_STEM.saturating_mul(length))
    } else {
        None
    };
    Some(match in_stem {
        Some(in_stem) if in_stem >= whole => (in_stem, note.stem),
        _ => (whole, 0),
    })
}

/// O(chars × term) despite the gap cap: a capped gap is the row's prefix max minus `GAP_MAX`.
fn fill(note: &Candidate, term: &[QueryChar], from: usize, table: &mut Vec<i32>) -> Option<i32> {
    let n = note.chars.len();
    table.clear();
    table.resize(term.len() * n, NONE);
    for (j, &q) in term.iter().enumerate() {
        let (above, rest) = table.split_at_mut(j * n);
        let row = &mut rest[..n];
        let prev = j.checked_sub(1).map(|up| &above[up * n..]);
        let (mut affine, mut prefix) = (NONE, NONE);
        for at in from..n {
            let reach = match prev {
                None => 0,
                Some(prev) => {
                    if at >= from + 2 {
                        affine = (affine - 1).max(prev[at - 2] - GAP_OPEN);
                        prefix = prefix.max(prev[at - 2]);
                    }
                    let consecutive = if at > from {
                        prev[at - 1] + CONSECUTIVE
                    } else {
                        NONE
                    };
                    consecutive.max(affine).max(prefix - GAP_MAX)
                }
            };
            if reachable(reach) && q.matches(note.chars[at]) {
                row[at] = reach + MATCH + i32::from(note.bonus[at]);
            }
        }
    }
    let last = term.len().checked_sub(1)? * n;
    table[last..]
        .iter()
        .copied()
        .max()
        .filter(|&best| reachable(best))
}

/// Whether a cell holds a real alignment, not a penalized `NONE`.
fn reachable(score: i32) -> bool {
    score > NONE / 2
}

fn gap(chars: usize) -> i32 {
    i32::try_from(chars).map_or(GAP_MAX, |chars| {
        GAP_OPEN.saturating_add(chars - 1).min(GAP_MAX)
    })
}

/// Walks `table` back from the best last cell, preferring a consecutive predecessor, and
/// pushes each matched char index.
fn backtrack(
    note: &Candidate,
    term: &[QueryChar],
    from: usize,
    table: &[i32],
    out: &mut Vec<usize>,
) {
    let n = note.chars.len();
    let last = term.len() - 1;
    let row = &table[last * n..];
    let Some(mut at) = (from..n)
        .filter(|&i| reachable(row[i]))
        .max_by_key(|&i| (row[i], std::cmp::Reverse(i)))
    else {
        return;
    };
    out.push(at);
    for j in (0..last).rev() {
        let reach = table[(j + 1) * n + at] - MATCH - i32::from(note.bonus[at]);
        let prev = &table[j * n..(j + 1) * n];
        let step = |k: usize| {
            if k + 1 == at {
                CONSECUTIVE
            } else {
                -gap(at - 1 - k)
            }
        };
        let Some(k) = (from..at)
            .rev()
            .find(|&k| reachable(prev[k]) && prev[k] + step(k) == reach)
        else {
            unreachable!("a filled cell has the predecessor that filled it");
        };
        at = k;
        out.push(at);
    }
}
