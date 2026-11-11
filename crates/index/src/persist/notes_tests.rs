use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use inkra_text::ByteOffset;
use inkra_vault::{EntryId, FileIdentity, Mtime, Stamp};
use proptest::prelude::*;

use super::header::seal;
use super::{CachedNote, Rejected, decode, encode, notes, words};
use crate::test_support::{ROOT, id, note_set, vault};
use crate::varint;
use crate::words::{NoteWords, Ordinal};
use crate::{FileRead, NoteContent, NoteIndex, Source, SourceLink, Style};

fn read(secs: u64, nanos: u32, size: u64, inode: u64) -> FileRead {
    FileRead {
        stamp: Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::new(secs, nanos)),
            size,
        },
        identity: FileIdentity { device: 7, inode },
    }
}

/// An index over `files`, each of `loads` loaded at its read; and each file's id.
fn index(files: &[&str], loads: &[(&str, &str, FileRead)]) -> (Vec<EntryId>, NoteIndex) {
    let (_fs, vault) = vault(files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    for (file, text, read) in loads {
        index.insert(id(&vault, file), NoteContent::parse(text, *read));
    }
    (files.iter().map(|file| id(&vault, file)).collect(), index)
}

fn decoded(bytes: &[u8]) -> Vec<CachedNote> {
    match decode(bytes, Path::new(ROOT)) {
        Ok(decoded) => decoded.notes,
        Err(rejected) => panic!("rejected: {}", rejected.reason()),
    }
}

fn cached(path: &str, text: &str, read: FileRead, ordinal: Option<u32>) -> CachedNote {
    CachedNote {
        path: path.into(),
        stamp: Some(read.stamp),
        identity: read.identity,
        ordinal: ordinal.map(Ordinal::new),
        source: Source::parse(text),
    }
}

#[test]
fn sources_by_path_across_shifted_ids() {
    let loads = [
        ("a.md", "See [[b]] and [[c#top]]", read(5, 1, 9, 1)),
        ("b.md", "Back to [a](a.md)", read(6, 2, 17, 2)),
    ];
    let (before, first) = index(&["a.md", "b.md"], &loads);
    let (after, second) = index(&["0.md", "a.md", "b.md"], &loads);
    assert_eq!(after[..2], before, "0.md takes a's id, a takes b's");
    let expected = [
        cached("a.md", loads[0].1, loads[0].2, Some(0)),
        cached("b.md", loads[1].1, loads[1].2, Some(1)),
    ];
    assert_eq!(
        decoded(&encode(&first.snapshot(), Path::new(ROOT)).expect("under the cap")),
        expected
    );
    assert_eq!(
        decoded(&encode(&second.snapshot(), Path::new(ROOT)).expect("under the cap")),
        expected
    );
}

fn name() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "0.md", "a.md", "b.md", "é.md", "d e.md", "z.md", "Ünï.md",
    ])
}

/// Link forms of every style, with and without a fragment, and multi-byte text.
fn piece() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "[[a]]",
        "[[A]]",
        "[x](a.md)",
        "[[b/c#h]]",
        "[[./d]]",
        "[[../e#é]]",
        "[x](f.md)",
        "[y](g%20h.md#k)",
        "[[#self]]",
        "[z](https://u.rs)",
        "[[Ünï cödé]]",
        "text ",
        "\n",
        "🎉",
        "l'été ",
        "éééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééé",
    ])
}

fn load() -> impl Strategy<Value = (String, FileRead)> {
    (
        prop::collection::vec(piece(), 0..12),
        0u64..1 << 40,
        0u32..1_000_000_000,
        any::<u64>(),
        any::<(u64, u64)>(),
    )
        .prop_map(|(pieces, secs, nanos, size, (device, inode))| {
            let mut read = read(secs, nanos, size, inode);
            read.identity.device = device;
            (pieces.concat(), read)
        })
}

proptest! {
    #[test]
    fn round_trip_records(notes in prop::collection::btree_map(name(), prop::option::of(load()), 1..7)) {
        let files: Vec<&str> = notes.keys().copied().collect();
        let loads: Vec<(&str, &str, FileRead)> = notes
            .iter()
            .filter_map(|(file, load)| load.as_ref().map(|(text, read)| (*file, text.as_str(), *read)))
            .collect();
        let (_, index) = index(&files, &loads);
        let decoded = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
        let mut minted = 0..;
        let expected: BTreeMap<Arc<str>, CachedNote> = loads
            .iter()
            .map(|(file, text, read)| {
                let ordinal = (!NoteWords::parse(text).is_empty()).then(|| minted.next()).flatten();
                (Arc::from(*file), cached(file, text, *read, ordinal))
            })
            .collect();
        prop_assert_eq!(decoded.len(), expected.len());
        let decoded: BTreeMap<Arc<str>, CachedNote> = decoded
            .into_iter()
            .map(|note| (Arc::clone(&note.path), note))
            .collect();
        prop_assert_eq!(decoded, expected);
    }
}

#[test]
fn two_links_to_one_path_share_their_arc() {
    let (_, index) = index(
        &["a.md", "c.md"],
        &[
            ("a.md", "[[b]] [[B]]", read(1, 0, 11, 1)),
            ("c.md", "[[b]]", read(2, 0, 5, 2)),
        ],
    );
    let notes = decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap"));
    let [a, c] = &notes[..] else {
        panic!("two records, not {}", notes.len());
    };
    let (b, upper, other) = (
        &a.source.links[0].path,
        &a.source.links[1].path,
        &c.source.links[0].path,
    );
    assert_eq!((&**b, &**upper, &**other), ("b", "B", "b"));
    assert!(Arc::ptr_eq(b, other), "one decode shares a path");
    assert!(!Arc::ptr_eq(b, upper), "the written form is the key");
}

#[test]
fn pre_epoch_writes_no_stamp() {
    let mut before = read(0, 0, 4, 3);
    before.stamp.mtime = Mtime(SystemTime::UNIX_EPOCH - Duration::from_nanos(1));
    let at_epoch = read(0, 0, 5, 4);
    let (_, index) = index(
        &["a.md", "b.md"],
        &[("a.md", "[[b]]", before), ("b.md", "[[a]]", at_epoch)],
    );
    let unstamped = CachedNote {
        path: "a.md".into(),
        stamp: None,
        identity: before.identity,
        ordinal: Some(Ordinal::new(0)),
        source: Source::default(),
    };
    assert_eq!(
        decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap")),
        [unstamped, cached("b.md", "[[a]]", at_epoch, Some(1))]
    );
}

#[test]
fn an_offset_past_u32_writes_no_stamp() {
    let (ids, mut index) = index(&["a.md"], &[]);
    let file = read(1, 0, 4, 3);
    let link = SourceLink {
        path: "b".into(),
        fragment: None,
        md: false,
        style: Style::Name,
        range: ByteOffset::new(0)..ByteOffset::new(1 << 32),
    };
    let content = NoteContent {
        source: Source {
            links: vec![link].into(),
        },
        words: NoteWords::parse(""),
        read: file,
    };
    index.insert(ids[0], content);
    let unstamped = CachedNote {
        path: "a.md".into(),
        stamp: None,
        identity: file.identity,
        ordinal: None,
        source: Source::default(),
    };
    assert_eq!(
        decoded(&encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap")),
        [unstamped]
    );
}

/// A section's bytes, written field by field with no check, so each rule can be broken alone.
#[derive(Default)]
pub(super) struct Raw(pub(super) Vec<u8>);

impl Raw {
    pub(super) fn var(mut self, value: u32) -> Self {
        varint::push(&mut self.0, value);
        self
    }

    pub(super) fn byte(mut self, byte: u8) -> Self {
        self.0.push(byte);
        self
    }

    pub(super) fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub(super) fn u64(mut self, value: u64) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub(super) fn str(self, bytes: &[u8]) -> Self {
        let Ok(len) = u32::try_from(bytes.len()) else {
            panic!("a short test string");
        };
        let mut raw = self.var(len);
        raw.0.extend_from_slice(bytes);
        raw
    }

    /// A record's path, its flags, the stamp `flags` asks for, device 7, inode 3 and no ordinal.
    fn head(self, path: &[u8], flags: u8, nanos: u32) -> Self {
        let raw = self.str(path).byte(flags);
        let raw = if flags & 1 == 1 {
            raw.u64(1).u32(nanos).u64(4)
        } else {
            raw
        };
        raw.u64(7).u64(3).var(0)
    }

    /// One link to `b` with no fragment.
    fn link(self, style: u8, range: (u32, u32)) -> Self {
        self.str(b"b").byte(0).byte(style).var(range.0).var(range.1)
    }

    /// One record of `a.md`, unstamped, holding one valid link.
    fn record(self) -> Self {
        self.head(b"a.md", 0, 0).var(1).link(2, (0, 0))
    }
}

/// Section 2 of an index with no words.
fn no_words() -> Vec<u8> {
    words::encode(&words::cached(&NoteIndex::default().words()).0)
}

fn sealed(section: &Raw) -> Vec<u8> {
    seal([ROOT.as_bytes(), &section.0, &no_words()]).expect("under the cap")
}

#[test]
fn hand_built_records_decode() {
    let link = SourceLink {
        path: "b".into(),
        fragment: None,
        md: false,
        style: Style::Relative,
        range: ByteOffset::new(0)..ByteOffset::new(0),
    };
    let identity = FileIdentity {
        device: 7,
        inode: 3,
    };
    let md_link = SourceLink {
        md: true,
        fragment: Some("h".into()),
        style: Style::Name,
        ..link.clone()
    };
    let md = Raw::default()
        .var(1)
        .head(b"a.md", 0, 0)
        .var(1)
        .str(b"b")
        .byte(3)
        .str(b"h")
        .byte(0)
        .var(0)
        .var(0);
    let stamped = Raw::default().var(1).head(b"a.md", 1, 999_999_999).var(0);
    let empty = Raw::default().var(1).head(b"", 0, 0).var(0);
    let cases = [
        (
            Raw::default().var(1).record(),
            CachedNote {
                path: "a.md".into(),
                stamp: None,
                identity,
                ordinal: None,
                source: Source {
                    links: vec![link.clone()].into(),
                },
            },
        ),
        (
            md,
            CachedNote {
                path: "a.md".into(),
                stamp: None,
                identity,
                ordinal: None,
                source: Source {
                    links: vec![md_link].into(),
                },
            },
        ),
        (
            stamped,
            CachedNote {
                path: "a.md".into(),
                stamp: Some(Stamp {
                    mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::new(1, 999_999_999)),
                    size: 4,
                }),
                identity,
                ordinal: None,
                source: Source::default(),
            },
        ),
        (
            empty,
            CachedNote {
                path: "".into(),
                stamp: None,
                identity,
                ordinal: None,
                source: Source::default(),
            },
        ),
    ];
    for (section, note) in cases {
        assert_eq!(decoded(&sealed(&section)), [note]);
    }
}

/// The smallest link record, last in the section: it pins `MIN_LINK_RECORD`.
#[test]
fn the_smallest_link_record_decodes() {
    let smallest = Raw::default()
        .var(1)
        .head(b"a.md", 0, 0)
        .var(1)
        .str(b"")
        .byte(0)
        .byte(2)
        .var(0)
        .var(0);

    let note = CachedNote {
        path: "a.md".into(),
        stamp: None,
        identity: FileIdentity {
            device: 7,
            inode: 3,
        },
        ordinal: None,
        source: Source {
            links: vec![SourceLink {
                path: "".into(),
                fragment: None,
                md: false,
                style: Style::Relative,
                range: ByteOffset::new(0)..ByteOffset::new(0),
            }]
            .into(),
        },
    };
    assert_eq!(decoded(&sealed(&smallest)), [note]);
}

#[test]
fn each_field_rejects() {
    let raw = Raw::default;
    let link = |style, range| raw().var(1).head(b"a.md", 0, 0).var(1).link(style, range);
    let cases = [
        (link(3, (0, 0)), "style"),
        (link(2, (5, 4)), "range"),
        (raw().var(2).record().record(), "duplicate"),
        (raw().var(1).head(b"a.md", 1, 1_000_000_000).var(0), "nanos"),
        (
            raw()
                .var(1)
                .str(b"a.md")
                .byte(1)
                .u64(u64::MAX)
                .u32(0)
                .u64(4),
            "mtime",
        ),
        (raw().var(1).record().byte(0), "trailing"),
        (raw().var(u32::MAX).record(), "count"),
        (raw().var(1).head(b"", 0, 0), "count"),
        (raw().var(1).head(b"a.md", 2, 0).var(0), "flags"),
        (raw().var(1).head(b"a.md", 3, 0).var(0), "flags"),
        (
            raw()
                .var(1)
                .head(b"a.md", 0, 0)
                .var(1)
                .str(b"b")
                .byte(4)
                .u32(0)
                .byte(0),
            "flags",
        ),
        (raw().var(1).head(b"a.md", 0, 0).var(u32::MAX), "links"),
        (raw().var(1).head(b"\xff", 0, 0).var(0), "utf8"),
        (
            raw().var(1).str(b"aaaaaaaaaaaaaaaaaaaa").byte(0).u64(7),
            "truncated",
        ),
        (raw().var(1).var(30).u64(0).u64(0).u32(0), "truncated"),
        (raw().byte(0x80), "varint"),
    ];
    for (section, expected) in cases {
        let Err(rejected) = decode(&sealed(&section), Path::new(ROOT)) else {
            panic!("{:?} decodes", section.0);
        };
        assert_eq!(rejected, Rejected::Corrupt(expected), "{:?}", section.0);
    }
}

#[test]
fn each_style_code_decodes_to_its_style() {
    for style in [Style::Name, Style::Suffix, Style::Relative] {
        assert_eq!(
            notes::style_from_code(notes::style_code(style)),
            Some(style)
        );
    }
    assert_eq!(notes::style_from_code(3), None);
}

#[test]
fn no_truncation_panics() {
    let loads = [
        ("a.md", "[[b#é]] and [x](c.md) 🎉", read(5, 7, 30, 1)),
        ("b.md", "[[a]]", read(6, 8, 5, 2)),
    ];
    let (_, index) = index(&["a.md", "b.md"], &loads);
    let section = notes::encode(&index.snapshot(), |_| None);
    for len in 0..section.len() {
        let bytes = seal([ROOT.as_bytes(), &section[..len], &no_words()]).expect("under the cap");
        assert!(decode(&bytes, Path::new(ROOT)).is_err(), "{len}");
    }
    let bytes = encode(&index.snapshot(), Path::new(ROOT)).expect("under the cap");
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len], Path::new(ROOT)).is_err(), "{len}");
    }
}
