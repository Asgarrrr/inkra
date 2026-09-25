//! `match_names` against a brute-force model that shares only D2's char rule.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use inkra_vault::{EntryId, Kind};
use proptest::prelude::*;
use proptest::sample::Index;
use unicode_normalization::UnicodeNormalization;

use super::fold::{PathChar, QueryChar};
use crate::key::strip_md;
use crate::test_support::{entries, vault};
use crate::{NameQuery, NoteSet, match_names, merge_names};

const CHARS: &[&str] = &[
    "a", "b", "k", "s", "e", "é", "e\u{301}", "É", "B", "\u{212A}", "ſ", "İ", "🎉", "-", "_", " ",
];

fn component() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(CHARS), 1..4)
        .prop_map(|parts| parts.concat())
        .prop_filter("no blank component", |part| !part.trim().is_empty())
}

fn files() -> impl Strategy<Value = Vec<String>> {
    let file = prop::collection::vec(component(), 1..4)
        .prop_map(|parts| format!("{}.md", parts.join("/")));
    prop::collection::btree_set(file, 1..12).prop_map(|set| set.into_iter().collect())
}

fn query() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(CHARS), 0..5).prop_map(|parts| parts.concat())
}

/// Every term a subsequence of the NFC path, by the shared char rule.
fn model_matches(path: &str, query: &str) -> bool {
    let path: Vec<PathChar> = path.chars().map(PathChar::new).collect();
    let query: String = query.nfc().collect();
    query.split_whitespace().all(|term| {
        let mut chars = path.iter();
        term.chars()
            .map(QueryChar::new)
            .all(|q| chars.any(|&c| q.matches(c)))
    })
}

fn notes(all: &[(EntryId, String, Kind)]) -> Vec<(EntryId, String)> {
    all.iter()
        .filter(|(_, _, kind)| *kind == Kind::Note)
        .map(|(id, path, _)| (*id, strip_md(path).nfc().collect()))
        .collect()
}

proptest! {
    #[test]
    fn match_names_finds_exactly_the_model_s_notes(files in files(), text in query()) {
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_fs, vault) = vault(&refs);
        let all = entries(&vault);
        let names = NoteSet::build(all.clone()).names();
        let query = NameQuery::new(&text, None, &[]);
        let found: BTreeSet<EntryId> =
            match_names(&names, 0..names.len(), &query, &AtomicBool::new(false), usize::MAX)
                .into_iter().map(|found| found.id).collect();
        let expected: BTreeSet<EntryId> = if query.is_empty() {
            BTreeSet::new()
        } else {
            notes(&all).into_iter().filter(|(_, path)| model_matches(path, &text)).map(|(id, _)| id).collect()
        };
        prop_assert_eq!(found, expected);
    }

    #[test]
    fn positions_are_ascending_char_starts_in_the_path(files in files(), text in query()) {
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_fs, vault) = vault(&refs);
        let names = NoteSet::build(entries(&vault)).names();
        let query = NameQuery::new(&text, None, &[]);
        for found in match_names(&names, 0..names.len(), &query, &AtomicBool::new(false), usize::MAX) {
            prop_assert!(found.positions.windows(2).all(|pair| pair[0] < pair[1]));
            for &at in &found.positions {
                prop_assert!(at < found.path.len() && found.path.is_char_boundary(at));
            }
        }
    }

    #[test]
    fn merging_any_split_equals_one_range(
        files in files(),
        text in query(),
        showing in any::<Index>(),
        recent in prop::collection::vec(any::<Index>(), 0..4),
        parts in 1usize..6,
        limit in 1usize..8,
    ) {
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_fs, vault) = vault(&refs);
        let all = notes(&entries(&vault));
        let names = NoteSet::build(entries(&vault)).names();
        let showing = Some(all[showing.index(all.len())].0);
        let recent: Vec<EntryId> = recent.iter().map(|at| all[at.index(all.len())].0).collect();
        let query = NameQuery::new(&text, showing, &recent);
        let cancel = AtomicBool::new(false);
        let whole = match_names(&names, 0..names.len(), &query, &cancel, limit);
        let split = names.chunks(parts).into_iter()
            .map(|range| match_names(&names, range, &query, &cancel, limit))
            .collect();
        prop_assert_eq!(merge_names(split, limit), whole);
    }
}

/// Spec D2's per-char bonus, recomputed from the settled constants.
fn model_bonus(raw: &[char], at: usize, stem: usize) -> i32 {
    let boundary = at == 0
        || matches!(raw[at - 1], '/' | '-' | '_' | ' ' | '.')
        || (raw[at - 1].is_lowercase() && raw[at].is_uppercase());
    i32::from(boundary) * 24 + i32::from(at == stem) * 32
}

/// Every increasing index sequence whose chars match `term`, in order.
fn alignments(
    path: &[PathChar],
    term: &[QueryChar],
    from: usize,
    out: &mut Vec<Vec<usize>>,
    cur: &mut Vec<usize>,
) {
    if cur.len() == term.len() {
        out.push(cur.clone());
        return;
    }
    for at in from..path.len() {
        if term[cur.len()].matches(path[at]) {
            cur.push(at);
            alignments(path, term, at + 1, out, cur);
            cur.pop();
        }
    }
}

fn alignment_score(raw: &[char], stem: usize, at: &[usize]) -> i32 {
    let mut total = 0;
    for (i, &p) in at.iter().enumerate() {
        total += 16 + model_bonus(raw, p, stem);
        if i > 0 {
            let g = p - at[i - 1] - 1;
            total += if g == 0 {
                16
            } else {
                -(i32::try_from(g).unwrap_or(i32::MAX).saturating_add(2)).min(12)
            };
        }
    }
    if at[0] >= stem {
        total += 8 * i32::try_from(at.len()).unwrap_or(i32::MAX);
    }
    total
}

/// The best score of `term` by exhaustive enumeration, with every alignment scoring it.
fn model_term(raw: &[char], term: &[QueryChar]) -> Option<(i32, Vec<Vec<usize>>)> {
    let stem = raw.iter().rposition(|&c| c == '/').map_or(0, |at| at + 1);
    let path: Vec<PathChar> = raw.iter().map(|&c| PathChar::new(c)).collect();
    let mut all = Vec::new();
    alignments(&path, term, 0, &mut all, &mut Vec::new());
    let best = all.iter().map(|at| alignment_score(raw, stem, at)).max()?;
    let best_at = all
        .into_iter()
        .filter(|at| alignment_score(raw, stem, at) == best)
        .collect();
    Some((best, best_at))
}

/// The query's terms as typed, repeats included.
fn model_typed(text: &str) -> Vec<Vec<QueryChar>> {
    let text: String = text.nfc().collect();
    text.split_whitespace()
        .map(|t| t.chars().map(QueryChar::new).collect())
        .collect()
}

/// The typed terms with each repeat after the first dropped.
fn model_unique(text: &str) -> Vec<Vec<QueryChar>> {
    let mut unique: Vec<Vec<QueryChar>> = Vec::new();
    for term in model_typed(text) {
        if !unique.contains(&term) {
            unique.push(term);
        }
    }
    unique
}

fn model_total(path: &str, text: &str, showing: bool) -> Option<i32> {
    let raw: Vec<char> = path.chars().collect();
    let stem = raw.iter().rposition(|&c| c == '/').map_or(0, |at| at + 1);
    let mut total = 0;
    for term in &model_unique(text) {
        total += model_term(&raw, term)?.0;
    }
    let joined: Vec<QueryChar> = model_typed(text).join(&QueryChar::new(' '));
    let stem_chars: Vec<PathChar> = raw[stem..].iter().map(|&c| PathChar::new(c)).collect();
    if stem_chars.len() == joined.len()
        && joined.iter().zip(&stem_chars).all(|(q, &c)| q.matches(c))
    {
        total += 1024;
    }
    if showing {
        total -= total / 4;
    }
    Some(total)
}

const SCORED: &[&str] = &[
    "a", "b", "A", "B", "é", "e", "E", "É", "e\u{301}", "İ", "\u{212A}", "k", "ſ", "🎉", "_", "-",
    " ", "/", ".",
];

fn scored_path() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(SCORED), 1..30)
        .prop_map(|parts| parts.concat())
        .prop_filter("a valid relative path", |path| {
            path.split('/')
                .all(|part| !part.trim().is_empty() && part != "." && !part.ends_with('.'))
        })
}

/// At most 10 chars, so the 64-char query cap never applies.
fn scored_query() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(
            &[
                "a", "b", "A", "e", "é", "E", "É", "e\u{301}", "i", "İ", "K", "\u{212A}", "ſ",
                "🎉", " ", "-", ".",
            ][..],
        ),
        1..6,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn scores_equal_the_best_alignment_by_brute_force(path in scored_path(), text in scored_query(), showing in any::<bool>()) {
        let file = format!("{path}.md");
        let (_fs, vault) = vault(&[file.as_str()]);
        let all = entries(&vault);
        let notes = notes(&all);
        prop_assume!(notes.len() == 1);
        let (id, nfc) = notes[0].clone();
        let names = NoteSet::build(all).names();
        let query = NameQuery::new(&text, showing.then_some(id), &[]);
        let found = match_names(&names, 0..names.len(), &query, &AtomicBool::new(false), usize::MAX);
        let expected = if query.is_empty() { None } else { model_total(&nfc, &text, showing) };
        prop_assert_eq!(found.first().map(|found| found.score), expected);
        let Some(found) = found.first() else { return Ok(()) };
        let raw: Vec<char> = nfc.chars().collect();
        let starts: Vec<usize> = nfc.char_indices().map(|(at, _)| at).collect();
        let indices: Vec<usize> = found.positions.iter().map(|at| starts.binary_search(at).unwrap_or(usize::MAX)).collect();
        if let [term] = model_unique(&text).as_slice() {
            let Some((_, best)) = model_term(&raw, term) else { unreachable!() };
            prop_assert!(best.contains(&indices), "positions {:?} are not a best alignment of {:?} in {:?}: {:?}", indices, text, nfc, best);
        }
    }
}
