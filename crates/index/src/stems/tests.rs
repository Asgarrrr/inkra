use std::collections::{BTreeMap, HashMap, HashSet};

use inkra_vault::{EntryId, Vault};
use proptest::prelude::*;

use super::Stems;
use crate::key::Key;
use crate::test_support::{id, link_target, vault};
use crate::{Source, Target};

const NOTES: [&str; 4] = ["n0.md", "n1.md", "n2.md", "n3.md"];

fn ids() -> (Vault, Vec<EntryId>) {
    let (_, vault) = vault(&NOTES);
    let ids = NOTES.iter().map(|path| id(&vault, path)).collect();
    (vault, ids)
}

/// Each row of `stems`, sorted, so the order sources arrived in does not count.
fn rows(stems: &Stems) -> HashMap<Key, Vec<(EntryId, u32)>> {
    stems
        .0
        .iter()
        .map(|(key, rows)| {
            let mut rows = rows.clone();
            rows.sort_unstable();
            (key.clone(), rows)
        })
        .collect()
}

#[test]
fn removing_a_stem_s_last_entry_drops_its_row() {
    let (_, ids) = ids();
    let (a, b) = (Source::parse("[[Plan]]"), Source::parse("[[f/plan]] [[x]]"));
    let mut stems = Stems::default();
    stems.insert(ids[0], &a);
    stems.insert(ids[1], &b);
    stems.remove(ids[0], &a);
    assert_eq!(stems.get(&Key::stem("plan")), Some(&[(ids[1], 0)][..]));
    stems.remove(ids[1], &b);
    assert_eq!(stems.get(&Key::stem("plan")), None);
    assert_eq!(stems, Stems::default());
}

#[test]
fn a_row_is_8_bytes() {
    let (_, ids) = ids();
    let mut stems = Stems::default();
    stems.insert(ids[0], &Source::parse("[[a]]"));
    let Some([row]) = stems.get(&Key::stem("a")) else {
        panic!("one row");
    };
    assert_eq!(size_of_val(row), 8);
}

#[test]
fn removing_every_gone_note_drops_their_rows() {
    let (_, ids) = ids();
    let (a, b) = (Source::parse("[[Plan]]"), Source::parse("[[plan]] [[x]]"));
    let mut stems = Stems::default();
    stems.insert(ids[0], &a);
    stems.insert(ids[1], &b);
    stems.remove_all(&HashSet::from([ids[0], ids[1]]), [&a, &b]);
    assert_eq!(stems, Stems::default());
}

#[test]
fn an_insert_then_its_remove_leaves_the_stems_as_before() {
    let (_, ids) = ids();
    let held = [
        Source::parse("[[Plan]] [[a]]"),
        Source::parse("[[f/plan]] [[Café]]"),
    ];
    let build = || {
        let mut stems = Stems::default();
        for (&id, source) in ids.iter().zip(&held) {
            stems.insert(id, source);
        }
        stems
    };
    let added = Source::parse("[[PLAN]] [[cafe\u{301}]] [[new]] [[plan]]");
    let mut stems = build();
    stems.insert(ids[2], &added);
    stems.remove(ids[2], &added);
    assert_eq!(stems, build());
    stems.insert(ids[2], &added);
    stems.remove_all(&HashSet::from([ids[2]]), [&added]);
    assert_eq!(stems, build());
}

#[derive(Debug, Clone)]
enum Op {
    /// A load or a reload of note `n`, as `NoteIndex::insert` does it.
    Insert(usize, String),
    Remove(usize),
    RemoveAll(Vec<usize>),
}

fn text() -> impl Strategy<Value = String> {
    let name = prop::sample::select(
        &[
            "a",
            "A",
            "f/a",
            "b.md",
            "cafe\u{301}",
            "Café",
            "#h",
            "https://x.org",
        ][..],
    );
    prop::collection::vec(name, 0..4).prop_map(|names| {
        names
            .iter()
            .map(|name| format!("[[{name}]]"))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn op() -> impl Strategy<Value = Op> {
    let n = 0..NOTES.len();
    prop_oneof![
        3 => (n.clone(), text()).prop_map(|(n, text)| Op::Insert(n, text)),
        1 => n.clone().prop_map(Op::Remove),
        1 => prop::collection::vec(n, 0..3).prop_map(Op::RemoveAll),
    ]
}

/// Every note link of the live sources under its stem, scanning each link's target.
fn model(ids: &[EntryId], live: &BTreeMap<usize, Source>) -> HashMap<Key, Vec<(EntryId, u32)>> {
    let mut rows: HashMap<Key, Vec<(EntryId, u32)>> = HashMap::new();
    for (&n, source) in live {
        for (at, link) in (0..).zip(source.links.iter()) {
            if let Target::Note { path, .. } = link_target(link) {
                rows.entry(Key::stem(&path)).or_default().push((ids[n], at));
            }
        }
    }
    for rows in rows.values_mut() {
        rows.sort_unstable();
    }
    rows
}

proptest! {
    #[test]
    fn stems_match_a_scan_of_the_live_sources(ops in prop::collection::vec(op(), 0..24)) {
        let (_, ids) = ids();
        let mut stems = Stems::default();
        let mut live: BTreeMap<usize, Source> = BTreeMap::new();
        for op in ops {
            match op {
                Op::Insert(n, text) => {
                    if let Some(old) = live.remove(&n) {
                        stems.remove(ids[n], &old);
                    }
                    let source = Source::parse(&text);
                    stems.insert(ids[n], &source);
                    live.insert(n, source);
                }
                Op::Remove(n) => {
                    if let Some(old) = live.remove(&n) {
                        stems.remove(ids[n], &old);
                    }
                }
                Op::RemoveAll(ns) => {
                    let gone: HashSet<EntryId> = ns.iter().map(|&n| ids[n]).collect();
                    let sources: Vec<Source> = ns.iter().filter_map(|n| live.remove(n)).collect();
                    stems.remove_all(&gone, &sources);
                }
            }
            prop_assert_eq!(rows(&stems), model(&ids, &live));
        }
    }
}
