//! The cases the `find` oracle draws: a vault, its loads and mtimes, and a query.

use proptest::prelude::*;
use proptest::sample::Index;

use std::time::SystemTime;

use inkra_vault::{EntryId, Kind};
use regex_syntax::is_word_character;
use unicode_normalization::UnicodeNormalization;

use crate::key::strip_md;
use crate::test_support::{PARTS, content, entries, mtime, vault};
use crate::words::{TERM_MAX_CHARS, cap, fold};
use crate::{NoteIndex, NoteQuery, NoteSet};

/// No `\n` or `.` (file-name rules); a skin tone modifier and U+0600 add grapheme edges.
fn component() -> impl Strategy<Value = String> {
    let parts: Vec<&str> = PARTS
        .iter()
        .copied()
        .filter(|part| !part.contains(['\n', '.']))
        .chain(["\u{1F3FB}", "\u{600}"])
        .collect();
    prop::collection::vec(prop::sample::select(parts), 1..4)
        .prop_map(|parts| parts.concat())
        .prop_filter("no blank component", |part| !part.trim().is_empty())
}

fn files() -> impl Strategy<Value = Vec<String>> {
    let file = prop::collection::vec(component(), 1..3)
        .prop_map(|parts| format!("{}.md", parts.join("/")));
    prop::collection::btree_set(file, 1..8).prop_map(|set| set.into_iter().collect())
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PARTS), 0..12).prop_map(|parts| parts.concat())
}

/// At most 3 parts of at most 20 chars: under S1's 64-char cut, which `words` tests pin.
fn query() -> impl Strategy<Value = String> {
    let parts: Vec<&str> = PARTS
        .iter()
        .copied()
        .filter(|part| part.chars().count() <= 20)
        .collect();
    prop::collection::vec(prop::sample::select(parts), 0..4).prop_map(|parts| parts.concat())
}

/// A vault and what the index is told of it; mtimes in `0..3` seconds, so they tie often.
#[derive(Debug, Clone)]
pub(crate) struct Case {
    pub(crate) files: Vec<String>,
    /// Per note in vault order (cycled): the scanned mtime.
    pub(crate) scanned: Vec<u64>,
    /// Per note in vault order (cycled): the loaded text and its mtime, or not loaded.
    pub(crate) loaded: Vec<Option<(String, u64)>>,
    pub(crate) query: String,
    /// Notes by vault order, modulo the note count.
    pub(crate) showing: Option<usize>,
    pub(crate) recent: Vec<usize>,
    /// Merge every fresh note into the segment before the snapshot.
    pub(crate) merged: bool,
}

/// A quarter type a stem next to a rival adding a word: only the exact name orders them.
pub(crate) fn case() -> impl Strategy<Value = Case> {
    (
        files(),
        prop::collection::vec(0..3_u64, 8),
        prop::collection::vec(prop::option::of((text(), 0..3_u64)), 8),
        query(),
        prop::option::weighted(0.25, any::<Index>()),
        prop::option::of(any::<usize>()),
        prop::collection::vec(any::<usize>(), 0..5),
        component(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(mut files, scanned, loaded, query, named, showing, recent, word, repeat, merged)| {
                let named = named.map(|at| {
                    let path = strip_md(&files[at.index(files.len())]);
                    let (folder, stem) = path.split_at(path.rfind('/').map_or(0, |at| at + 1));
                    let rival = format!("{stem} {}", if repeat { stem } else { &word });
                    (
                        format!("{folder}{rival}.md"),
                        if repeat { rival } else { stem.to_owned() },
                    )
                });
                let query = match named {
                    Some((rival, typed)) if typed.chars().count() <= 64 => {
                        if !files.contains(&rival) {
                            files.push(rival);
                        }
                        typed
                    }
                    _ => query,
                };
                Case {
                    files,
                    scanned,
                    loaded,
                    query,
                    showing,
                    recent,
                    merged,
                }
            },
        )
}

pub(crate) struct Note {
    pub(crate) id: EntryId,
    /// NFC, `.md` stripped.
    pub(crate) path: String,
    pub(crate) text: Option<String>,
    pub(crate) modified: SystemTime,
}

/// A case's index, its notes in vault order, and its query.
pub(crate) struct Built {
    pub(crate) index: NoteIndex,
    pub(crate) notes: Vec<Note>,
    pub(crate) query: NoteQuery,
    pub(crate) showing: Option<EntryId>,
    pub(crate) recent: Vec<EntryId>,
}

pub(crate) fn build(case: &Case) -> Built {
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
            index.insert(note.id, content(text.as_str(), mtime(*n)));
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
        notes,
        showing,
        recent,
    }
}

/// Each word of `text` with its byte offset, raw and folded; words folding to nothing dropped.
pub(crate) fn words(text: &str) -> Vec<(usize, &str, String)> {
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
pub(crate) fn typed(query: &str) -> Vec<String> {
    words(query)
        .into_iter()
        .map(|(_, _, folded)| cap(&folded, TERM_MAX_CHARS).to_owned())
        .collect()
}

/// S1's terms: the typed words, distinct.
pub(crate) fn terms(typed: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for term in typed {
        if !terms.contains(term) {
            terms.push(term.clone());
        }
    }
    terms
}

pub(crate) fn split(len: usize, cuts: &[Index]) -> Vec<std::ops::Range<usize>> {
    let mut at: Vec<usize> = cuts.iter().map(|cut| cut.index(len + 1)).collect();
    at.push(0);
    at.push(len);
    at.sort_unstable();
    at.windows(2).map(|pair| pair[0]..pair[1]).collect()
}
