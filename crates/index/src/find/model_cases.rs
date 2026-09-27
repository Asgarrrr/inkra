//! The cases the `find` oracle draws: a vault, its loads and mtimes, and a query.

use proptest::prelude::*;
use proptest::sample::Index;

use crate::key::strip_md;
use crate::test_support::PARTS;

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
pub(super) struct Case {
    pub(super) files: Vec<String>,
    /// Per note in vault order (cycled): the scanned mtime.
    pub(super) scanned: Vec<u64>,
    /// Per note in vault order (cycled): the loaded text and its mtime, or not loaded.
    pub(super) loaded: Vec<Option<(String, u64)>>,
    pub(super) query: String,
    /// Notes by vault order, modulo the note count.
    pub(super) showing: Option<usize>,
    pub(super) recent: Vec<usize>,
}

/// A quarter type a stem next to a rival adding a word: only the exact name orders them.
pub(super) fn case() -> impl Strategy<Value = Case> {
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
    )
        .prop_map(
            |(mut files, scanned, loaded, query, named, showing, recent, word, repeat)| {
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
                }
            },
        )
}
