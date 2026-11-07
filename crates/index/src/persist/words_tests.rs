use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use inkra_vault::{EntryId, Vault};
use proptest::prelude::*;

use super::bytes::narrow;
use super::header::seal;
use super::notes_tests::Raw;
use super::{Decoded, Rejected, decode, encode};
use crate::test_support::{ROOT, find_all, id, indexed, load, note_set, vault};
use crate::words::{Ordinal, Ordinals, Words};
use crate::{NoteIndex, NoteQuery};

const FILES: &[&str] = &["a.md", "b.md", "c.md", "d.md", "e.md", "f.md"];
const WORDS: &[&str] = &["plan", "plane", "été", "ab", "b", "ζ", "Plan"];

#[derive(Debug, Clone)]
enum Op {
    Load(usize, String),
    Remove(usize),
    Merge,
}

fn op() -> impl Strategy<Value = Op> {
    let words = prop::collection::vec(prop::sample::select(WORDS), 0..4);
    prop_oneof![
        3 => (0..FILES.len(), words).prop_map(|(file, words)| Op::Load(file, words.join(" "))),
        1 => (0..FILES.len()).prop_map(Op::Remove),
        1 => Just(Op::Merge),
    ]
}

/// An index over `vault` after each of `ops`, in order.
fn built(vault: &Vault, ops: &[Op]) -> NoteIndex {
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(vault));
    for op in ops {
        match op {
            Op::Load(file, text) => load(&mut index, id(vault, FILES[*file]), text),
            Op::Remove(file) => index.remove(id(vault, FILES[*file])),
            Op::Merge => index.merge_all(),
        }
    }
    index
}

fn decoded(bytes: &[u8]) -> Decoded {
    match decode(bytes, Path::new(ROOT)) {
        Ok(decoded) => decoded,
        Err(rejected) => panic!("rejected: {}", rejected.reason()),
    }
}

/// The store `decoded` holds, each record's ordinal named by the id at its place in `ids`.
fn restored(decoded: Decoded, ids: &[EntryId]) -> Words {
    let ordinals: Ordinals = decoded
        .notes
        .iter()
        .zip(ids)
        .filter_map(|(note, &id)| Some((id, note.ordinal?)))
        .collect();
    let words = decoded.words;
    Words::restore(
        words.segment,
        words.alive,
        ordinals,
        words.next,
        words.total,
    )
}

fn ids_by_path(decoded: &Decoded, vault: &Vault) -> Vec<EntryId> {
    decoded
        .notes
        .iter()
        .map(|note| id(vault, &note.path))
        .collect()
}

/// `entries` keyed by path, so stores over two vaults compare.
fn by_path(
    entries: BTreeMap<(String, EntryId), u32>,
    vault: &Vault,
) -> BTreeMap<(String, String), u32> {
    let paths: HashMap<EntryId, String> = vault
        .entries()
        .map(|entry| {
            (
                entry.id,
                entry.path.as_path().to_string_lossy().into_owned(),
            )
        })
        .collect();
    entries
        .into_iter()
        .map(|((word, id), count)| ((word, paths[&id].clone()), count))
        .collect()
}

proptest! {
    #[test]
    fn round_trip_equals_entries(ops in prop::collection::vec(op(), 0..24)) {
        let (_fs, before) = vault(FILES);
        let index = built(&before, &ops);
        let decoded = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
        let shifted: Vec<&str> = ["0.md"].iter().chain(FILES).copied().collect();
        let (_fs, after) = vault(&shifted);
        let ids = ids_by_path(&decoded, &after);
        let words = restored(decoded, &ids);
        prop_assert_eq!(
            by_path(words.snapshot().entries(), &after),
            by_path(index.words().entries(), &before)
        );
    }

    #[test]
    fn reseal_never_panics_after_ok(
        ops in prop::collection::vec(op(), 0..24),
        flips in prop::collection::vec((any::<prop::sample::Index>(), 1..=u8::MAX), 1..=8),
    ) {
        let (_fs, before) = vault(FILES);
        let mut bytes = encode(&built(&before, &ops).snapshot(), Path::new(ROOT)).expect("under the cap");
        for (at, flip) in flips {
            let at = 24 + at.index(bytes.len() - 24);
            bytes[at] ^= flip;
        }
        let crc = crc32fast::hash(&bytes[24..]);
        bytes[20..24].copy_from_slice(&crc.to_le_bytes());
        if let Ok(decoded) = decode(&bytes, Path::new(ROOT)) {
            let files: Vec<String> = (0..decoded.notes.len()).map(|n| format!("n{n}.md")).collect();
            let files: Vec<&str> = files.iter().map(String::as_str).collect();
            let (_fs, numbered) = vault(&files);
            let ids: Vec<EntryId> = files.iter().map(|file| id(&numbered, file)).collect();
            let mut words = restored(decoded, &ids);
            let snapshot = words.snapshot();
            for (word, _) in snapshot.entries().keys() {
                for end in word.char_indices().map(|(at, _)| at).skip(1).chain([word.len()]) {
                    let query = NoteQuery::new(&word[..end], None, &[]);
                    let Some(lookup) = snapshot.lookup(&query, &AtomicBool::new(false)) else {
                        panic!("not cancelled");
                    };
                    for term in 0..lookup.term_count() {
                        for &id in &ids {
                            let _ = lookup.get(term, id);
                        }
                    }
                }
            }
            words.merge_all();
            let _ = words.snapshot().entries();
        }
    }

    #[test]
    fn bm25_unchanged(ops in prop::collection::vec(op(), 0..24)) {
        let (_fs, vault) = vault(FILES);
        let index = built(&vault, &ops);
        let decoded = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
        let ids = ids_by_path(&decoded, &vault);
        let restored = built(&vault, &ops).with_words(restored(decoded, &ids));
        for text in WORDS.iter().chain(&["pla", "é", "plan b", "ab plane"]) {
            let query = NoteQuery::new(text, None, &[]);
            prop_assert_eq!(
                find_all(&restored.snapshot(), &query, 10),
                find_all(&index.snapshot(), &query, 10)
            );
        }
    }
}

/// Each of the first `loaded` files loaded, merged, then each of `removed` removed; and its file.
fn sparse(loaded: usize, removed: &[usize]) -> (Vault, NoteIndex, Decoded) {
    let (_fs, vault) = vault(FILES);
    let mut ops: Vec<Op> = (0..loaded)
        .map(|file| Op::Load(file, WORDS[file].to_owned()))
        .collect();
    ops.push(Op::Merge);
    ops.extend(removed.iter().map(|&file| Op::Remove(file)));
    let index = built(&vault, &ops);
    let decoded = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
    (vault, index, decoded)
}

fn ordinals(decoded: &Decoded) -> Vec<(&str, Option<Ordinal>)> {
    decoded
        .notes
        .iter()
        .map(|note| (&*note.path, note.ordinal))
        .collect()
}

#[test]
fn compacts_when_sparse() {
    let (vault, index, decoded) = sparse(5, &[0, 2, 3]);
    assert_eq!(decoded.words.next, 2);
    assert_eq!(
        decoded.words.alive.iter().collect::<Vec<_>>(),
        [Ordinal::new(0), Ordinal::new(1)]
    );
    assert_eq!(
        ordinals(&decoded),
        [
            ("b.md", Some(Ordinal::new(0))),
            ("e.md", Some(Ordinal::new(1)))
        ]
    );
    let ids = ids_by_path(&decoded, &vault);
    assert_eq!(
        restored(decoded, &ids).snapshot().entries(),
        index.words().entries()
    );
    let (_, _, half) = sparse(6, &[0, 2, 4]);
    assert_eq!(half.words.next, 6, "next == 2 * alive keeps every ordinal");
    assert_eq!(
        ordinals(&half),
        [
            ("b.md", Some(Ordinal::new(1))),
            ("d.md", Some(Ordinal::new(3))),
            ("f.md", Some(Ordinal::new(5))),
        ]
    );
}

/// Section 2 field by field, with section 1's record ordinals, so each rule breaks alone.
struct Fields {
    next: u32,
    total: u64,
    bits: u32,
    alive: Vec<u64>,
    notes: u32,
    bytes: &'static [u8],
    bounds: Vec<u32>,
    starts: Vec<u32>,
    postings: Vec<u8>,
    lengths: Vec<u32>,
    trailing: &'static [u8],
    /// Each record's ordinal as written, 0 for none; record `n` is `n{n}.md`.
    records: Vec<u32>,
}

/// Words `ab` (ordinal 0 once, 1 twice) and `b` (1 once), each ordinal alive and named.
fn valid() -> Fields {
    Fields {
        next: 2,
        total: 4,
        bits: 2,
        alive: vec![0b11],
        notes: 2,
        bytes: b"abb",
        bounds: vec![0, 2, 3],
        starts: vec![0, 4, 6],
        postings: vec![0, 1, 1, 2, 1, 1],
        lengths: vec![1, 3],
        trailing: b"",
        records: vec![1, 2],
    }
}

fn u32s(raw: Raw, values: &[u32]) -> Raw {
    values
        .iter()
        .fold(raw.var(narrow(values.len())), |raw, &value| raw.u32(value))
}

impl Fields {
    fn notes(&self) -> Raw {
        let raw = Raw::default().var(narrow(self.records.len()));
        (0..).zip(&self.records).fold(raw, |raw, (n, &ordinal)| {
            raw.str(format!("n{n}.md").as_bytes())
                .byte(0)
                .u64(7)
                .u64(3)
                .var(ordinal)
                .var(0)
        })
    }

    fn words(&self) -> Raw {
        let raw = Raw::default().var(self.next).u64(self.total).var(self.bits);
        let raw = self.alive.iter().fold(raw, |raw, &word| raw.u64(word));
        let raw = u32s(raw.var(self.notes).str(self.bytes), &self.bounds);
        let raw = u32s(u32s(raw, &self.starts).str(&self.postings), &self.lengths);
        self.trailing.iter().fold(raw, |raw, &byte| raw.byte(byte))
    }

    fn sealed(&self) -> Vec<u8> {
        seal([ROOT.as_bytes(), &self.notes().0, &self.words().0]).expect("under the cap")
    }
}

fn rejects(fields: &Fields, expected: &'static str) {
    let Err(rejected) = decode(&fields.sealed(), Path::new(ROOT)) else {
        panic!("{:?} decodes", fields.words().0);
    };
    assert_eq!(
        rejected,
        Rejected::Corrupt(expected),
        "{:?}",
        fields.words().0
    );
}

fn rejects_words(words: &Raw, expected: &'static str) {
    let bytes = seal([ROOT.as_bytes(), &valid().notes().0, &words.0]).expect("under the cap");
    let Err(rejected) = decode(&bytes, Path::new(ROOT)) else {
        panic!("{:?} decodes", words.0);
    };
    assert_eq!(rejected, Rejected::Corrupt(expected), "{:?}", words.0);
}

#[test]
fn hand_built_words_decode() {
    let decoded = decoded(&valid().sealed());
    let (_fs, vault) = vault(&["n0.md", "n1.md"]);
    let (n0, n1) = (id(&vault, "n0.md"), id(&vault, "n1.md"));
    assert_eq!(
        restored(decoded, &[n0, n1]).snapshot().entries(),
        BTreeMap::from([
            (("ab".to_owned(), n0), 1),
            (("ab".to_owned(), n1), 2),
            (("b".to_owned(), n1), 1),
        ])
    );
}

#[test]
fn a_truncated_varint_is_varint() {
    rejects_words(&Raw::default().byte(0x80), "varint");
    let postings = vec![0, 1, 1, 2, 1, 0x80];
    rejects(
        &Fields {
            postings,
            ..valid()
        },
        "varint",
    );
}

#[test]
fn a_cut_section_is_truncated() {
    rejects_words(&Raw::default().var(2).byte(0), "truncated");
    let fields = Fields {
        next: u32::MAX,
        bits: u32::MAX,
        ..valid()
    };
    rejects(&fields, "truncated");
    let section = valid().words().0;
    for len in 0..section.len() {
        let bytes =
            seal([ROOT.as_bytes(), &valid().notes().0, &section[..len]]).expect("under the cap");
        assert!(decode(&bytes, Path::new(ROOT)).is_err(), "{len}");
    }
}

#[test]
fn alive_bits_other_than_next_are_alive() {
    rejects(&Fields { bits: 3, ..valid() }, "alive");
    rejects(&Fields { bits: 1, ..valid() }, "alive");
}

#[test]
fn a_bit_past_next_is_alive() {
    for word in [0b111, 0b11 | 1 << 63] {
        let alive = vec![word];
        rejects(&Fields { alive, ..valid() }, "alive");
    }
}

#[test]
fn bytes_not_utf8_are_utf8() {
    let bytes = b"a\xffb";
    rejects(&Fields { bytes, ..valid() }, "utf8");
}

#[test]
fn bounds_must_cut_the_bytes_into_words() {
    for bounds in [
        vec![0, 2, 4],
        vec![1, 2, 3],
        vec![0, 2, 2, 3],
        vec![0, 3, 2, 3],
        vec![],
    ] {
        rejects(&Fields { bounds, ..valid() }, "bounds");
    }
    let split = Fields {
        bytes: "éb".as_bytes(),
        bounds: vec![0, 1, 3],
        ..valid()
    };
    rejects(&split, "bounds");
}

#[test]
fn words_out_of_order_are_sorted() {
    for (bytes, bounds) in [(&b"bab"[..], vec![0, 1, 3]), (&b"abab"[..], vec![0, 2, 4])] {
        rejects(
            &Fields {
                bytes,
                bounds,
                ..valid()
            },
            "sorted",
        );
    }
}

#[test]
fn starts_must_cut_the_postings_per_word() {
    for starts in [
        vec![0, 4],
        vec![1, 4, 6],
        vec![0, 4, 5],
        vec![0, 6, 6],
        vec![0, 4, 4, 6],
    ] {
        rejects(&Fields { starts, ..valid() }, "starts");
    }
}

#[test]
fn a_zero_gap_after_the_first_is_gap() {
    let postings = vec![0, 1, 0, 2, 1, 1];
    rejects(
        &Fields {
            postings,
            ..valid()
        },
        "gap",
    );
}

#[test]
fn a_gap_past_u32_is_overflow() {
    let fields = Fields {
        postings: vec![1, 1, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 1, 1, 1],
        starts: vec![0, 8, 10],
        ..valid()
    };
    rejects(&fields, "overflow");
}

#[test]
fn an_ordinal_not_minted_is_ordinal() {
    let postings = vec![0, 1, 2, 2, 1, 1];
    rejects(
        &Fields {
            postings,
            ..valid()
        },
        "ordinal",
    );
}

#[test]
fn a_zero_count_is_frequency() {
    let postings = vec![0, 0, 1, 2, 1, 1];
    rejects(
        &Fields {
            postings,
            ..valid()
        },
        "frequency",
    );
}

#[test]
fn notes_other_than_held_are_notes() {
    rejects(
        &Fields {
            notes: 3,
            ..valid()
        },
        "notes",
    );
    rejects(
        &Fields {
            notes: 1,
            ..valid()
        },
        "notes",
    );
}

#[test]
fn an_alive_ordinal_not_held_is_unheld() {
    let fields = Fields {
        next: 3,
        bits: 3,
        alive: vec![0b111],
        records: vec![1, 2, 3],
        ..valid()
    };
    rejects(&fields, "unheld");
}

#[test]
fn lengths_past_next_are_lengths() {
    let lengths = vec![1, 3, 0];
    rejects(&Fields { lengths, ..valid() }, "lengths");
}

#[test]
fn bytes_after_the_lengths_are_trailing() {
    let trailing = b"\0";
    rejects(
        &Fields {
            trailing,
            ..valid()
        },
        "trailing",
    );
}

#[test]
fn a_record_naming_a_dead_ordinal_is_dead() {
    let fields = Fields {
        next: 3,
        bits: 3,
        records: vec![1, 3],
        ..valid()
    };
    rejects(&fields, "dead");
}

#[test]
fn two_records_naming_one_ordinal_are_shared() {
    let records = vec![1, 1];
    rejects(&Fields { records, ..valid() }, "shared");
}

#[test]
fn an_alive_ordinal_no_record_names_is_unnamed() {
    for records in [vec![1, 0], vec![2]] {
        rejects(&Fields { records, ..valid() }, "unnamed");
    }
}

#[test]
fn total_other_than_the_live_lengths_is_total() {
    rejects(
        &Fields {
            total: 5,
            ..valid()
        },
        "total",
    );
    let dead_length = Fields {
        alive: vec![0b10],
        records: vec![0, 2],
        ..valid()
    };
    rejects(&dead_length, "total");
}

#[test]
fn a_full_last_alive_word_decodes() {
    let (_, index) = indexed(&[Some("plan"); 64]);
    let decoded = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
    assert_eq!(decoded.words.next, 64);
    assert_eq!(decoded.words.alive.len(), 64);
}

#[test]
fn an_alive_note_of_length_zero_is_length() {
    rejects(
        &Fields {
            total: 0,
            lengths: vec![0, 0],
            ..valid()
        },
        "length",
    );
}
