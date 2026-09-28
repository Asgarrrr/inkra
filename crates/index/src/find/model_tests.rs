//! `find_notes` and `merge_notes` against a naive model sharing only `fold` (S1).

use std::cmp::Reverse;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use inkra_vault::{EntryId, Kind};
use proptest::prelude::*;
use proptest::sample::Index;
use regex_syntax::is_word_character;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use super::model_cases::{self, Case};
use crate::key::strip_md;
use crate::test_support::{entries, mtime, vault};
use crate::words::{TERM_MAX_CHARS, bm25, cap, fold, idf};
use crate::{
    Group, NoteContent, NoteIndex, NoteMatch, NoteQuery, NoteSet, find_notes, merge_notes,
};

struct Note {
    id: EntryId,
    path: String,
    text: Option<String>,
    modified: SystemTime,
}

struct Built {
    index: NoteIndex,
    notes: Vec<Note>,
    query: NoteQuery,
    expected: Vec<(EntryId, Group)>,
}

fn build(case: &Case) -> Built {
    let refs: Vec<&str> = case.files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&refs);
    let mut all = entries(&vault);
    let mut notes = Vec::new();
    for (id, path, kind, scanned) in &mut all {
        if *kind != Kind::Note {
            continue;
        }
        let k = notes.len() % 8;
        *scanned = mtime(case.scanned[k]);
        let loaded = case.loaded[k].clone();
        notes.push(Note {
            id: *id,
            path: strip_md(path).nfc().collect(),
            modified: loaded.as_ref().map_or(scanned.0, |(_, n)| mtime(*n).0),
            text: loaded.map(|(text, _)| text),
        });
    }
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(all));
    for (k, note) in notes.iter().enumerate() {
        if let Some((text, n)) = &case.loaded[k % 8] {
            index.insert(note.id, NoteContent::parse(text.as_str().into(), mtime(*n)));
        }
    }
    if case.merged {
        index.merge_all();
    }
    let showing = case.showing.map(|at| notes[at % notes.len()].id);
    let recent: Vec<EntryId> = case
        .recent
        .iter()
        .map(|at| notes[at % notes.len()].id)
        .collect();
    Built {
        index,
        query: NoteQuery::new(&case.query, showing, &recent),
        expected: model(&notes, &case.query, showing, &recent),
        notes,
    }
}

/// Each word of `text` with its byte offset, raw and folded; words folding to nothing dropped.
fn words(text: &str) -> Vec<(usize, &str, String)> {
    let mut found = Vec::new();
    let mut at = 0;
    for word in text.split(|c: char| !is_word_character(c)) {
        let folded = fold(word);
        if !folded.is_empty() {
            found.push((at, word, folded.into_owned()));
        }
        at += word.len();
        at += text[at..].chars().next().map_or(0, char::len_utf8);
    }
    found
}

/// S1's words of a query under the 64-char cut: split, folded, capped, repeats kept.
fn typed(query: &str) -> Vec<String> {
    words(query)
        .into_iter()
        .map(|(_, _, folded)| cap(&folded, TERM_MAX_CHARS).to_owned())
        .collect()
}

/// S1's terms: the typed words, distinct.
fn terms(typed: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for term in typed {
        if !terms.contains(term) {
            terms.push(term.clone());
        }
    }
    terms
}

/// BM25's figures over the notes with words: how many, their mean length, each term's df.
struct Corpus {
    notes: u32,
    avg: f64,
    df: Vec<u32>,
}

fn small(n: usize) -> u32 {
    let Ok(n) = u32::try_from(n) else {
        panic!("a drawn vault is small");
    };
    n
}

fn corpus(notes: &[Note], terms: &[String]) -> Corpus {
    let texts: Vec<Vec<(usize, &str, String)>> = notes
        .iter()
        .filter_map(|note| note.text.as_deref().map(words))
        .filter(|text| !text.is_empty())
        .collect();
    let holds = |text: &[(usize, &str, String)], term: &str| {
        text.iter().any(|(_, _, word)| word.starts_with(term))
    };
    let total = small(texts.iter().map(Vec::len).sum());
    Corpus {
        notes: small(texts.len()),
        avg: f64::from(total) / f64::from(small(texts.len())),
        df: terms
            .iter()
            .map(|term| small(texts.iter().filter(|text| holds(text, term)).count()))
            .collect(),
    }
}

/// Key 2 as its bits: a non-negative float orders as its bits do.
type Key = (
    Group,
    Reverse<bool>,
    Reverse<usize>,
    Reverse<u64>,
    bool,
    usize,
    Reverse<SystemTime>,
    usize,
    usize,
);

/// S3's key of the note at `order`, or `None` when it is not listed.
fn key(
    note: &Note,
    order: usize,
    typed: &[String],
    terms: &[String],
    corpus: &Corpus,
    showing: Option<EntryId>,
    recent: &[EntryId],
) -> Option<Key> {
    let rank = recent.iter().position(|id| *id == note.id);
    let (group, exact, whole, weight) = if terms.is_empty() {
        rank?;
        if showing == Some(note.id) {
            return None;
        }
        (Group::Name, false, 0, 0.0)
    } else {
        let stem = note.path.rfind('/').map_or(0, |at| at + 1);
        let name = words(&note.path);
        let stem_words: Vec<&str> = name
            .iter()
            .filter(|(at, _, _)| *at >= stem)
            .map(|(_, _, word)| word.as_str())
            .collect();
        let exact = stem_words == typed;
        let text = note.text.as_deref().map(words).unwrap_or_default();
        let len = small(text.len());
        let (mut in_name, mut whole, mut in_stem, mut score) = (0, 0, 0, 0.0);
        for (at, term) in terms.iter().enumerate() {
            let matched: Vec<_> = name
                .iter()
                .filter(|(_, _, word)| word.starts_with(term.as_str()))
                .collect();
            if matched.is_empty() {
                let hits: Vec<_> = text
                    .iter()
                    .filter(|(_, _, word)| word.starts_with(term.as_str()))
                    .collect();
                if hits.is_empty() {
                    return None;
                }
                whole += usize::from(hits.iter().any(|(_, _, word)| word == term));
                let idf = idf(corpus.df[at], corpus.notes);
                score += bm25(idf, small(hits.len()), len, corpus.avg);
            } else {
                in_name += 1;
                whole += usize::from(matched.iter().any(|(_, _, word)| word == term));
                in_stem += u32::from(matched.iter().any(|(at, _, _)| *at >= stem));
            }
        }
        if in_name == terms.len() {
            (Group::Name, exact, whole, f64::from(in_stem))
        } else if in_name == 0 {
            (Group::Text, exact, whole, score)
        } else {
            (Group::NameAndText, exact, whole, score)
        }
    };
    Some((
        group,
        Reverse(exact),
        Reverse(whole),
        Reverse(weight.to_bits()),
        showing == Some(note.id),
        rank.unwrap_or(usize::MAX),
        Reverse(note.modified),
        note.path.chars().count(),
        order,
    ))
}

fn model(
    notes: &[Note],
    query: &str,
    showing: Option<EntryId>,
    recent: &[EntryId],
) -> Vec<(EntryId, Group)> {
    let typed = typed(query);
    let terms = terms(&typed);
    let corpus = corpus(notes, &terms);
    let mut listed: Vec<(Key, EntryId)> = notes
        .iter()
        .enumerate()
        .filter_map(|(order, note)| {
            let key = key(note, order, &typed, &terms, &corpus, showing, recent)?;
            Some((key, note.id))
        })
        .collect();
    listed.sort_unstable();
    listed.into_iter().map(|(key, id)| (id, key.0)).collect()
}

fn listed(found: &[NoteMatch]) -> Vec<(EntryId, Group)> {
    found.iter().map(|found| (found.id, found.group)).collect()
}

fn split(len: usize, cuts: &[Index]) -> Vec<std::ops::Range<usize>> {
    let mut at: Vec<usize> = cuts.iter().map(|cut| cut.index(len + 1)).collect();
    at.push(0);
    at.push(len);
    at.sort_unstable();
    at.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

/// Every grapheme start of `text`, and its end.
fn bounds(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect()
}

/// S3's highlights: per word and term, from its first grapheme to where its run folds to the term.
fn model_highlights(path: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let bounds = bounds(path);
    let mut lit = Vec::new();
    for (at, word, folded) in words(path) {
        for term in terms {
            if !folded.starts_with(term.as_str()) {
                continue;
            }
            let Some(&start) = bounds.iter().rev().find(|&&b| b <= at) else {
                panic!("0 is a bound");
            };
            let Some(&end) = bounds.iter().find(|&&e| {
                e > at && fold(&path[at..e.min(at + word.len())]).starts_with(term.as_str())
            }) else {
                panic!("the whole word folds to a start of {term:?}");
            };
            lit.push((start, end));
        }
    }
    lit.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in lit {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// One range, then `cuts`' ranges merged under `limit`, list what the model lists.
pub(super) fn lists_what_the_model_lists(
    case: &Case,
    cuts: &[Index],
    limit: usize,
) -> Result<(), TestCaseError> {
    let Built {
        index,
        query,
        expected,
        ..
    } = build(case);

    let not_cancelled = AtomicBool::new(false);
    let snapshot = index.snapshot();
    let Some(lookup) = snapshot.lookup(&query, &not_cancelled) else {
        panic!("not cancelled");
    };
    let all = 0..index.names().len();
    let kept = find_notes(
        &snapshot,
        &lookup,
        all.clone(),
        &query,
        &not_cancelled,
        usize::MAX,
    );
    let found = merge_notes(&snapshot, vec![kept], &query, usize::MAX);
    prop_assert_eq!(listed(&found), expected.clone());

    let chunks = split(all.len(), cuts)
        .into_iter()
        .map(|range| find_notes(&snapshot, &lookup, range, &query, &not_cancelled, limit))
        .collect();
    let merged = merge_notes(&snapshot, chunks, &query, limit);
    prop_assert_eq!(
        listed(&merged),
        expected[..limit.min(expected.len())].to_vec()
    );
    prop_assert_eq!(&merged[..], &found[..limit.min(found.len())]);
    Ok(())
}

/// The highlights are whole path graphemes, ascending, never touching, and exactly S3's.
pub(super) fn highlights_are_word_prefixes(case: &Case) -> Result<(), TestCaseError> {
    let Built {
        index,
        notes,
        query,
        ..
    } = build(case);
    let terms = terms(&typed(&case.query));
    let not_cancelled = AtomicBool::new(false);
    let snapshot = index.snapshot();
    let Some(lookup) = snapshot.lookup(&query, &not_cancelled) else {
        panic!("not cancelled");
    };
    let all = 0..index.names().len();
    let kept = find_notes(&snapshot, &lookup, all, &query, &not_cancelled, usize::MAX);
    let found = merge_notes(&snapshot, vec![kept], &query, usize::MAX);
    for row in &found {
        let Some(note) = notes.iter().find(|note| note.id == row.id) else {
            panic!("a listed note is a note");
        };
        prop_assert_eq!(&*row.path, note.path.as_str());
        let bounds = bounds(&note.path);
        for range in &row.name_highlights {
            prop_assert!(range.start < range.end, "{:?}", row.name_highlights);
            prop_assert!(bounds.contains(&range.start) && bounds.contains(&range.end));
        }
        for pair in row.name_highlights.windows(2) {
            prop_assert!(pair[0].end < pair[1].start, "{:?}", row.name_highlights);
        }
        let lit: Vec<(usize, usize)> = row
            .name_highlights
            .iter()
            .map(|range| (range.start, range.end))
            .collect();
        prop_assert_eq!(
            lit,
            model_highlights(&note.path, &terms),
            "{:?} for {:?}",
            note.path,
            terms
        );
    }
    Ok(())
}

proptest! {
    #[test]
    fn find_and_merge_list_what_the_model_lists(
        case in model_cases::case(),
        cuts in prop::collection::vec(any::<Index>(), 0..3),
        limit in 0..10_usize,
    ) {
        lists_what_the_model_lists(&case, &cuts, limit)?;
    }

    #[test]
    fn the_highlights_are_word_prefixes(case in model_cases::case()) {
        highlights_are_word_prefixes(&case)?;
    }
}
