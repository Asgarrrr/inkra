//! `lookup` and the ordinals against a model that re-reads every text. It shares `fold` with
//! the code under test (S1) and splits words with its own `str::split`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::AtomicBool;

use proptest::prelude::*;
use regex_syntax::is_word_character;

use inkra_vault::EntryId;

use super::{NoteWords, TERM_MAX_CHARS, Words, WordsMerge, cap, fold};
use crate::test_support::{PARTS, id, vault};
use crate::{Lookup, NoteQuery, TermHit, WordsSnapshot};

fn text(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PARTS), 0..len).prop_map(|parts| parts.concat())
}

/// The folded, capped, distinct terms of `query`, as S1 defines them.
fn model_terms(query: &str) -> Vec<String> {
    fn push(terms: &mut Vec<String>, word: &str) {
        let folded = fold(word);
        let term = cap(&folded, TERM_MAX_CHARS).to_owned();
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    let chars: Vec<char> = query.chars().collect();
    let typed: String = chars.iter().take(64).collect();
    let split = chars.len() > 64 && is_word_character(chars[63]) && is_word_character(chars[64]);
    let mut words: Vec<&str> = typed.split(|c: char| !is_word_character(c)).collect();
    let last = if split { words.pop() } else { None };
    let mut terms: Vec<String> = Vec::new();
    for word in words {
        push(&mut terms, word);
    }
    if let Some(last) = last
        && terms.is_empty()
    {
        push(&mut terms, last);
    }
    terms
}

/// Every whole word of `text`, folded and never capped.
fn model_words(text: &str) -> Vec<String> {
    text.split(|c: char| !is_word_character(c))
        .map(|word| fold(word).into_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

fn model_hit(text: &str, term: &str) -> Option<TermHit> {
    let mut hit = TermHit {
        count: 0,
        whole: false,
    };
    for word in model_words(text) {
        if word.starts_with(term) {
            hit.count += 1;
            hit.whole |= word == term;
        }
    }
    (hit.count > 0).then_some(hit)
}

#[derive(Debug, Clone)]
enum Op {
    Set(usize, String),
    Remove(usize),
    RemoveAll(Vec<usize>),
    Merge,
    StartMerge,
    ApplyMerge,
    DropMerge,
    Hold,
}

fn op(notes: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..notes, text(60)).prop_map(|(n, text)| Op::Set(n, text)),
        1 => (0..notes).prop_map(Op::Remove),
        1 => prop::collection::vec(0..notes, 0..notes).prop_map(Op::RemoveAll),
        1 => Just(Op::Merge),
        2 => Just(Op::StartMerge),
        2 => Just(Op::ApplyMerge),
        2 => Just(Op::DropMerge),
        1 => Just(Op::Hold),
    ]
}

/// Every `(word, note)` of the model's texts and its count, as `NoteWords` stores them.
fn model_entries(ids: &[EntryId], model: &[Option<String>]) -> BTreeMap<(String, EntryId), u32> {
    ids.iter()
        .zip(model)
        .filter_map(|(id, text)| Some((*id, text.as_deref()?)))
        .flat_map(|(id, text)| {
            NoteWords::parse(text)
                .iter()
                .map(|(word, count)| ((word.to_owned(), id), count))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every (term, note) answer of `found`, in order.
fn answers(found: &Lookup, ids: &[EntryId], terms: usize) -> Vec<Option<TermHit>> {
    (0..terms)
        .flat_map(|t| ids.iter().map(move |id| found.get(t, *id)))
        .collect()
}

proptest! {
    #[test]
    fn fold_is_idempotent_on_every_word(text in text(12)) {
        for word in model_words(&text) {
            prop_assert_eq!(fold(&word), word.as_str());
        }
    }

    /// Across writes, removes and merges, some started before writes and applied after them,
    /// `lookup` answers as the model does, and a held snapshot keeps its first answer.
    #[test]
    fn lookup_agrees_with_a_model(
        ops in prop::collection::vec(op(6), 0..32),
        query_text in text(12),
    ) {
        let files: Vec<String> = (0..6).map(|n| format!("n{n}.md")).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_fs, vault) = vault(&files);
        let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
        let mut words = Words::default();
        let mut running: Option<WordsMerge> = None;
        let mut model: Vec<Option<String>> = vec![None; ids.len()];
        // Minted at a note's first words, kept through an empty text, freed by a remove.
        let mut ordinals: Vec<Option<u32>> = vec![None; ids.len()];
        let mut next = 0;
        let mut held = Vec::new();
        let query = NoteQuery::new(&query_text, None, &[]);
        let terms = model_terms(&query_text);
        let not_cancelled = AtomicBool::new(false);
        let lookup = |snapshot: &WordsSnapshot| {
            let Some(found) = snapshot.lookup(&query, &not_cancelled) else {
                panic!("not cancelled");
            };
            found
        };
        for op in ops {
            match op {
                Op::Set(n, text) => {
                    words.set(ids[n], NoteWords::parse(&text));
                    if ordinals[n].is_none() && !model_words(&text).is_empty() {
                        ordinals[n] = Some(next);
                        next += 1;
                    }
                    model[n] = Some(text);
                }
                Op::Remove(n) => {
                    words.remove(ids[n]);
                    model[n] = None;
                    ordinals[n] = None;
                }
                Op::RemoveAll(notes) => {
                    let gone: HashSet<EntryId> = notes.iter().map(|&n| ids[n]).collect();
                    words.remove_all(&gone);
                    for n in notes {
                        model[n] = None;
                        ordinals[n] = None;
                    }
                }
                Op::Merge => words.merge_all(),
                Op::StartMerge => {
                    let started = words.start_merge();
                    prop_assert!(running.is_none() || started.is_none(), "one merge at a time");
                    running = running.or(started);
                }
                Op::ApplyMerge => {
                    if let Some(merge) = running.take() {
                        let _ = words.apply(merge.run());
                    }
                }
                Op::DropMerge => drop(running.take()),
                Op::Hold => {
                    let snapshot = words.snapshot();
                    let answer = answers(&lookup(&snapshot), &ids, terms.len());
                    let entries = snapshot.entries();
                    held.push((snapshot, answer, entries));
                }
            }
            prop_assert_eq!(words.snapshot().entries(), model_entries(&ids, &model));
            let held_ordinals: HashMap<EntryId, u32> = words
                .snapshot()
                .ordinals
                .iter()
                .map(|(&id, ordinal)| (id, ordinal.get()))
                .collect();
            let model_ordinals: HashMap<EntryId, u32> = ids
                .iter()
                .zip(&ordinals)
                .filter_map(|(&id, ordinal)| Some((id, (*ordinal)?)))
                .collect();
            prop_assert_eq!(held_ordinals, model_ordinals);
            let found = lookup(&words.snapshot());
            prop_assert_eq!(found.term_count(), terms.len());
            for (t, term) in terms.iter().enumerate() {
                for (id, text) in ids.iter().zip(&model) {
                    let expected = text.as_deref().and_then(|text| model_hit(text, term));
                    prop_assert_eq!(found.get(t, *id), expected, "term {:?} in {:?}", term, text);
                }
            }
            for (snapshot, answer, entries) in &held {
                let again = answers(&lookup(snapshot), &ids, terms.len());
                prop_assert_eq!(&again, answer);
                prop_assert_eq!(&snapshot.entries(), entries);
            }
        }
    }
}
