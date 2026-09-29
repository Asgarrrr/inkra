//! `search` against a naive word-start model, and against `find_notes`: the two boxes agree.

use std::ops::Range;
use std::sync::atomic::AtomicBool;

use inkra_text::ByteOffset;
use inkra_vault::EntryId;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

use super::tests::searched;
use crate::find::model_cases::{self, Built, Case, build, split, terms, typed, words};
use crate::find::{boundaries, prefix};
use crate::test_support::{find_all, indexed};
use crate::{Group, HITS_PER_NOTE, Hit, NoteHits, Searched};

/// Whether every term starts a folded word of `text`; never for no term.
fn holds(text: &str, terms: &[String]) -> bool {
    let words = words(text);
    !terms.is_empty()
        && terms
            .iter()
            .all(|term| words.iter().any(|(_, _, word)| word.starts_with(term)))
}

/// Each word a term starts, lit by `prefix` for the longest such term, touching ranges merged;
/// every hit of the note, with no cap.
fn model_hits(text: &str, terms: &[String]) -> Vec<Range<usize>> {
    let bounds = boundaries(text);
    let mut lit: Vec<Range<usize>> = Vec::new();
    for (at, word, folded) in words(text) {
        let Some(term) = terms
            .iter()
            .filter(|term| folded.starts_with(term.as_str()))
            .max_by_key(|term| term.len())
        else {
            continue;
        };
        let range = prefix(text, &bounds, at..at + word.len(), term);
        match lit.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => lit.push(range),
        }
    }
    lit
}

/// The model's hits over `texts` in order, and the `Searched` it predicts under `limit`.
fn model(texts: &[(EntryId, Option<&str>)], query: &str, limit: usize) -> Searched {
    let terms = terms(&typed(query));
    let mut notes = Vec::new();
    let mut room = limit;
    let mut total = 0;
    for (id, text) in texts {
        let Some(text) = text.filter(|text| holds(text, &terms)) else {
            continue;
        };
        let every = model_hits(text, &terms);
        total += every.len().min(HITS_PER_NOTE);
        let kept = every.len().min(HITS_PER_NOTE).min(room);
        room -= kept;
        if kept > 0 {
            notes.push(NoteHits {
                note: *id,
                hits: every[..kept]
                    .iter()
                    .map(|range| {
                        let line = text[..range.start].matches('\n').count() + 1;
                        Hit::new(text, ByteOffset(range.start)..ByteOffset(range.end), line)
                    })
                    .collect(),
                capped: kept < every.len(),
            });
        }
    }
    Searched {
        notes,
        limit_reached: total >= limit,
    }
}

/// What a drawn case exercised, so the oracle is not vacuous.
#[derive(Default)]
struct Drawn {
    matched: usize,
    missed: usize,
    text_rows: usize,
    /// Notes the prefilter rejects while holding some terms, not all.
    partial: usize,
}

/// Hits and notes equal the model's, and `find_notes` agrees.
fn agrees(case: &Case, cuts: &[Index], limit: usize) -> Result<Drawn, TestCaseError> {
    let Built {
        index,
        notes,
        query,
        ..
    } = build(case);
    let snapshot = index.snapshot();
    let cancel = AtomicBool::new(false);
    let texts: Vec<(EntryId, Option<&str>)> = notes
        .iter()
        .map(|note| (note.id, note.text.as_deref()))
        .collect();

    let found = searched(&snapshot, 0..notes.len(), &case.query, &cancel, usize::MAX);
    prop_assert_eq!(&found, &model(&texts, &case.query, usize::MAX));

    let parts = split(notes.len(), cuts)
        .into_iter()
        .map(|range| searched(&snapshot, range, &case.query, &cancel, limit))
        .collect();
    prop_assert_eq!(
        Searched::merge(parts, limit),
        model(&texts, &case.query, limit)
    );

    let hit: Vec<EntryId> = found.notes.iter().map(|group| group.note).collect();
    let rows = find_all(&snapshot, &query, usize::MAX);
    for row in &rows {
        prop_assert!(
            row.group != Group::Text || hit.contains(&row.id),
            "a text row without a hit"
        );
    }
    for id in &hit {
        prop_assert!(rows.iter().any(|row| row.id == *id), "a hit without a row");
    }
    let terms = terms(&typed(&case.query));
    let loaded: Vec<&str> = texts.iter().filter_map(|(_, text)| *text).collect();
    let searchable = if terms.is_empty() { 0 } else { loaded.len() };
    let partial = loaded
        .iter()
        .filter(|text| {
            let words = words(text);
            let holds = |term: &String| words.iter().any(|(_, _, word)| word.starts_with(term));
            terms.iter().any(holds) && !terms.iter().all(holds)
        })
        .count();
    Ok(Drawn {
        matched: hit.len(),
        missed: searchable - hit.len(),
        text_rows: rows.iter().filter(|row| row.group == Group::Text).count(),
        partial,
    })
}

/// Cases whose query is often cut from loaded texts: from a word start it matches, from inside
/// a word it tests the word-start rule; two cuts from two notes often split the notes.
fn case() -> impl Strategy<Value = Case> {
    let cut = || (any::<Index>(), any::<Index>(), 1..8_usize);
    let cuts = prop::collection::vec(cut(), 1..3);
    (model_cases::case(), prop::option::weighted(0.8, cuts)).prop_map(|(mut case, cuts)| {
        let texts: Vec<&str> = case
            .loaded
            .iter()
            .take(case.files.len())
            .flatten()
            .map(|(text, _)| text.as_str())
            .filter(|text| !text.is_empty())
            .collect();
        if let Some(cuts) = cuts
            && !texts.is_empty()
        {
            let cut = |(note, at, chars): &(Index, Index, usize)| {
                let text = texts[note.index(texts.len())];
                let starts: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
                text[starts[at.index(starts.len())]..]
                    .chars()
                    .take(*chars)
                    .collect::<String>()
            };
            case.query = cuts.iter().map(cut).collect::<Vec<_>>().join(" ");
        }
        case
    })
}

const WORDY: &[&str] = &[
    "a", "a", "a", "-", "é", "e\u{301}", "_", "½", " ", "\n", "🎉", "1", "\u{301}", "A", "ab",
];

fn text(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(WORDY), 0..len).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn the_two_boxes_agree(
        case in case(),
        cuts in prop::collection::vec(any::<Index>(), 0..3),
        limit in 0..10_usize,
    ) {
        agrees(&case, &cuts, limit)?;
    }

    #[test]
    fn hits_equal_the_model_past_the_per_note_cap(
        texts in prop::collection::vec(prop::option::weighted(0.8, text(400)), 0..5),
        query in text(4),
        limit in 0..400_usize,
    ) {
        let refs: Vec<Option<&str>> = texts.iter().map(Option::as_deref).collect();
        let (ids, index) = indexed(&refs);
        let found = searched(&index.snapshot(), 0..ids.len(), &query, &AtomicBool::new(false), limit);
        let texts: Vec<(EntryId, Option<&str>)> = ids.into_iter().zip(refs).collect();
        prop_assert_eq!(found, model(&texts, &query, limit));
    }
}

#[test]
fn the_drawn_cases_hold_notes_matched_and_notes_missed() {
    let mut runner = TestRunner::deterministic();
    let mut total = Drawn::default();
    for _ in 0..256 {
        let Ok(case) = case().new_tree(&mut runner) else {
            panic!("the strategy draws a case");
        };
        let Ok(drawn) = agrees(&case.current(), &[], usize::MAX) else {
            panic!("the property holds");
        };
        total.matched += drawn.matched;
        total.missed += drawn.missed;
        total.text_rows += drawn.text_rows;
        total.partial += drawn.partial;
    }
    let Drawn {
        matched,
        missed,
        text_rows,
        partial,
    } = total;
    assert!(matched >= 50, "{matched} notes matched");
    assert!(missed >= 50, "{missed} notes missed");
    assert!(text_rows >= 40, "{text_rows} text rows");
    assert!(partial >= 30, "{partial} notes holding some terms");
}
