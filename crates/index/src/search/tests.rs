use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use inkra_vault::{EntryId, Stamp};

use crate::test_support::{indexed, load, mtime, search_texts, text_map};
use crate::{HITS_PER_NOTE, Hit, IndexSnapshot, NoteIndex, NoteQuery, Searched};

fn run(texts: &[Option<&str>], text: &str, limit: usize) -> Searched {
    let (ids, index) = indexed(texts);
    all(&index, &text_map(&ids, texts), text, limit)
}

fn all(
    index: &NoteIndex,
    texts: &HashMap<EntryId, Arc<str>>,
    text: &str,
    limit: usize,
) -> Searched {
    let query = NoteQuery::new(text, None, &[]);
    search_texts(&index.snapshot(), texts, &query, limit)
}

/// Each hit with its note, in order.
fn hits(found: &Searched) -> impl Iterator<Item = (EntryId, &Hit)> {
    found
        .notes
        .iter()
        .flat_map(|group| group.hits.iter().map(move |hit| (group.note, hit)))
}

/// The notes cut short, in order.
fn capped(found: &Searched) -> Vec<EntryId> {
    found
        .notes
        .iter()
        .filter(|group| group.capped)
        .map(|group| group.note)
        .collect()
}

/// Each hit as its note's place in `texts` and the text it lights.
fn lit<'a>(texts: &[Option<&'a str>], query: &str) -> Vec<(usize, &'a str)> {
    let (ids, index) = indexed(texts);
    hits(&all(&index, &text_map(&ids, texts), query, usize::MAX))
        .map(|(note, hit)| {
            let Some(at) = ids.iter().position(|id| *id == note) else {
                panic!("a hit names a note of the index");
            };
            let Some(text) = texts[at] else {
                panic!("a hit is in a loaded note");
            };
            (at, &text[hit.range.start.get()..hit.range.end.get()])
        })
        .collect()
}

fn lit_in<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
    lit(&[Some(text)], query)
        .into_iter()
        .map(|(_, lit)| lit)
        .collect()
}

#[test]
fn hits_come_in_vault_order_and_skip_notes_not_loaded() {
    let texts = [Some("x plan"), None, Some("plan plan")];
    let (ids, index) = indexed(&texts);
    let found = all(&index, &text_map(&ids, &texts), "plan", 10);
    let at: Vec<(EntryId, usize)> = hits(&found)
        .map(|(note, hit)| (note, hit.range.start.get()))
        .collect();
    assert_eq!(at, [(ids[0], 2), (ids[2], 0), (ids[2], 5)]);
    assert!(!found.limit_reached);
}

#[test]
fn a_folded_term_finds_every_spelling() {
    for text in ["été", "Été", "e\u{301}te", "ETE"] {
        assert_eq!(lit_in(text, "ete"), [text], "{text:?}");
    }
    assert_eq!(lit_in("e\u{301}te", "été"), ["e\u{301}te"], "NFC over NFD");
    assert_eq!(lit_in("été", "e\u{301}te"), ["été"], "NFD over NFC");
    assert_eq!(lit_in("ΟΔΥΣΣΕΥΣ", "οδυσσευς"), ["ΟΔΥΣΣΕΥΣ"]);
}

#[test]
fn a_term_matches_only_at_a_word_start() {
    assert_eq!(lit_in("un projet", "jet"), Vec::<&str>::new());
    assert_eq!(lit_in("un projet", "proj"), ["proj"]);
    assert_eq!(lit_in("l'Été", "ét"), ["Ét"]);
}

#[test]
fn every_term_must_start_a_word_in_any_order_and_place() {
    let text = "le projet avance\nla réunion est demain";
    assert_eq!(lit_in(text, "réunion projet"), ["projet", "réunion"]);
    assert_eq!(
        lit(&[Some("le projet"), Some(text)], "projet réunion"),
        [(1, "projet"), (1, "réunion")],
        "a note missing one term gives no hit"
    );
}

#[test]
fn a_query_without_a_term_finds_nothing() {
    for query in ["", "   ", "[[", "-- !"] {
        assert_eq!(
            run(&[Some("[[link]] -- ! a")], query, 10),
            Searched::default(),
            "{query:?}"
        );
    }
}

#[test]
fn a_hit_is_whole_graphemes_of_the_saved_text() {
    assert_eq!(lit_in("🎉\u{301}x", "x"), ["🎉\u{301}x"]);
    assert_eq!(lit_in("a\u{1F3FB}", "a"), ["a\u{1F3FB}"]);
}

#[test]
fn a_hit_touching_the_last_one_at_the_limit_still_merges_into_it() {
    // One grapheme spans `a` and the mark starting `\u{301}b`: their hits touch.
    let text = "a\u{1F3FB}\u{301}b";
    let found = run(&[Some(text)], "a b", 1);
    let ranges: Vec<(usize, usize)> = hits(&found)
        .map(|(_, hit)| (hit.range.start.get(), hit.range.end.get()))
        .collect();
    assert_eq!(ranges, [(0, text.len())]);
}

#[test]
fn a_word_two_terms_start_gives_one_hit_for_the_longer() {
    assert_eq!(lit_in("projet", "pro proj"), ["proj"]);
    assert_eq!(lit_in("projet", "proj pro"), ["proj"]);
}

#[test]
fn a_note_gives_at_most_100_hits_and_is_capped_past_them() {
    let many = "a ".repeat(150);
    let texts = [Some(many.as_str()), Some("a")];
    let (ids, index) = indexed(&texts);
    let found = all(&index, &text_map(&ids, &texts), "a", 1_000);
    assert_eq!(hits(&found).count(), HITS_PER_NOTE + 1);
    assert!(!found.limit_reached);
    assert_eq!(capped(&found), [ids[0]]);
}

#[test]
fn a_note_with_exactly_100_hits_is_not_capped() {
    let exact = "a ".repeat(HITS_PER_NOTE);
    let found = run(&[Some(&exact)], "a", 1_000);
    assert_eq!(hits(&found).count(), HITS_PER_NOTE);
    assert_eq!(capped(&found), []);
}

#[test]
fn a_note_cut_by_the_limit_is_capped() {
    let texts = [Some("a a a"), Some("a a")];
    let (ids, index) = indexed(&texts);
    let texts = text_map(&ids, &texts);
    assert_eq!(capped(&all(&index, &texts, "a", 4)), [ids[1]]);
    assert_eq!(
        capped(&all(&index, &texts, "a", 3)),
        [],
        "exactly its room of hits"
    );
    assert_eq!(capped(&all(&index, &texts, "a", 2)), [ids[0]]);
}

#[test]
fn a_hit_knows_the_line_of_its_start() {
    let line_of = |text: &str| -> Vec<usize> {
        hits(&run(&[Some(text)], "a", 10))
            .map(|(_, hit)| hit.line)
            .collect()
    };
    assert_eq!(line_of("a\nb a\n\n a"), [1, 2, 4]);
    assert_eq!(
        line_of(&format!("{}\na", "é".repeat(40))),
        [2],
        "multibyte before"
    );
}

#[test]
fn the_search_stops_at_the_limit() {
    let found = run(&[Some("a a a"), Some("a a")], "a", 4);
    assert_eq!(hits(&found).count(), 4);
    assert!(found.limit_reached);
    let exact = run(&[Some("a a"), Some("a a")], "a", 4);
    assert!(
        exact.limit_reached,
        "exactly `limit` hits reaches the limit"
    );
    assert!(run(&[Some("a")], "a", 0).limit_reached);
}

#[test]
fn a_flag_set_before_the_lookup_returns_nothing() {
    let (_, index) = indexed(&[Some("a"), Some("a")]);
    let set = AtomicBool::new(true);
    assert!(
        index
            .snapshot()
            .lookup(&NoteQuery::new("a", None, &[]), &set)
            .is_none()
    );
}

#[test]
fn a_snapshot_pairs_the_words_and_the_stamp_of_one_moment() {
    let (ids, mut index) = indexed(&[Some("old words")]);
    let before = index.snapshot();
    load(&mut index, ids[0], "new text");
    let after = index.snapshot();
    let stamps = |snapshot: &IndexSnapshot, query: &str| -> Vec<Stamp> {
        let query = NoteQuery::new(query, None, &[]);
        let Some(candidates) = snapshot.candidates(&query, &AtomicBool::new(false)) else {
            panic!("not cancelled");
        };
        candidates.iter().map(|candidate| candidate.stamp).collect()
    };
    let loaded = |size| Stamp {
        mtime: mtime(0),
        size,
    };
    assert_eq!(stamps(&before, "old"), [loaded(9)]);
    assert_eq!(stamps(&before, "new"), []);
    assert_eq!(stamps(&after, "new"), [loaded(8)]);
    assert_eq!(stamps(&after, "old"), []);
}

fn context_of(text: &str, query: &str) -> (String, String) {
    let found = run(&[Some(text)], query, 1);
    let Some(Hit {
        context,
        context_match,
        ..
    }) = found.notes.into_iter().flat_map(|group| group.hits).next()
    else {
        panic!("{query:?} in {text:?}");
    };
    let matched = context[context_match].to_owned();
    (context, matched)
}

#[test]
fn context_keeps_60_bytes_before_and_100_after_within_the_line() {
    let text = format!("{} HIT {}", "b".repeat(70), "a".repeat(120));
    let (context, matched) = context_of(&text, "hit");
    assert_eq!(matched, "HIT");
    assert_eq!(
        context,
        format!("{} HIT {}", "b".repeat(59), "a".repeat(99))
    );

    let (context, _) = context_of("first\nsay hit here\nlast", "hit");
    assert_eq!(context, "say hit here");
}
