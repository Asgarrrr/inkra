use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::time::{Duration, SystemTime};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Entry, FileIdentity, Kind, Mtime, Stamp, Vault};

use crate::persist::{Decoded, decode, encode};
use crate::seed::Seed;
use crate::test_support::{ROOT, id, note_set};
use crate::{FileRead, NoteContent, NoteIndex};

/// A second and a half after the epoch: a sub-second mtime, as a real disk writes.
fn fine() -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_millis(1_500))
}

/// Before the epoch: the cache writes no stamp for it.
fn early() -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH - Duration::from_secs(1))
}

fn inode(inode: u64) -> FileIdentity {
    FileIdentity { device: 1, inode }
}

/// A scanned vault of `files`, each at its inode.
fn scanned(files: &[(&str, u64)]) -> Vault {
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for &(file, ino) in files {
        fs.insert_file_with_identity(format!("{ROOT}/{file}"), "", inode(ino));
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new(ROOT))) else {
        panic!("a MemFs scan cannot fail");
    };
    vault
}

/// The cache of an index holding each `(file, text, inode, mtime)` as loaded.
fn cached(notes: &[(&str, &str, u64, Mtime)]) -> Decoded {
    // Its own inodes: a vault collapses hard links, and two cached notes may share one.
    let files: Vec<(&str, u64)> = (1_000..)
        .zip(notes)
        .map(|(ino, &(file, ..))| (file, ino))
        .collect();
    let vault = scanned(&files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    for &(file, text, ino, mtime) in notes {
        let read = FileRead {
            stamp: Stamp {
                mtime,
                size: text.len() as u64,
            },
            identity: inode(ino),
        };
        index.insert(id(&vault, file), NoteContent::parse(text, read));
    }
    match decode(&encode(&index, Path::new(ROOT)), Path::new(ROOT)) {
        Ok(decoded) => decoded,
        Err(rejected) => panic!("rejected: {}", rejected.reason()),
    }
}

fn seed(decoded: Decoded, vault: &Vault) -> Seed {
    seed_over(decoded, vault.entries(), vault)
}

/// The seed over `entries`, `vault`'s own or edited.
fn seed_over<'a>(
    decoded: Decoded,
    entries: impl Iterator<Item = &'a Entry>,
    vault: &Vault,
) -> Seed {
    let notes = entries.filter(|entry| entry.kind == Kind::Note);
    Seed::build(decoded, notes, &note_set(vault))
}

/// What the seed holds for one note: its words, its read's inode, whether `stamps` lists it.
type Seeded = (Vec<String>, Option<u64>, bool);

/// Each seeded note by path.
fn seeded(seed: &Seed, vault: &Vault) -> BTreeMap<String, Seeded> {
    let corpus = seed.corpus.snapshot();
    let words = seed.words.snapshot().entries();
    let notes = note_set(vault);
    seed.sources
        .keys()
        .map(|&id| {
            let Some(entry) = vault.entry(id) else {
                panic!("a seeded note is a vault note");
            };
            let held = words
                .keys()
                .filter(|(_, note)| *note == id)
                .map(|(word, _)| word.clone())
                .collect();
            let read = corpus.read(notes.notes.order(id));
            let listed = seed.stamps.iter().any(|&(listed, _)| listed == id);
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (path, (held, read.map(|read| read.identity.inode), listed))
        })
        .collect()
}

fn note(words: &[&str], ino: u64) -> Seeded {
    let words = words.iter().map(|word| (*word).to_owned()).collect();
    (words, Some(ino), true)
}

#[test]
fn path_then_inode_one_to_one() {
    let decoded = cached(&[
        ("a.md", "alpha", 10, fine()),
        ("b.md", "beta", 11, fine()),
        ("c.md", "gamma [[b]]", 12, fine()),
    ]);
    // `b.md` now holds `a`'s inode, but its path claims it for `b`; `c` moved to `z.md`.
    let vault = scanned(&[("b.md", 10), ("y.md", 13), ("z.md", 12)]);

    let seed = seed(decoded, &vault);

    let expected = BTreeMap::from([
        ("b.md".to_owned(), note(&["beta"], 11)),
        ("z.md".to_owned(), note(&["b", "gamma"], 12)),
    ]);
    assert_eq!(seeded(&seed, &vault), expected);
    assert_eq!(seed.words.snapshot().total, 3, "a's length is gone");
    let z = id(&vault, "z.md");
    let stems: Vec<_> = seed.by_stem.values().flatten().collect();
    assert_eq!(stems, [&(z, 0)], "the link is keyed by its new id");
}

#[test]
fn ambiguous_inode_matches_nothing() {
    let decoded = cached(&[
        ("a.md", "alpha", 20, fine()),
        ("b.md", "beta", 20, fine()),
        ("c.md", "gamma", 21, fine()),
    ]);
    let vault = scanned(&[("x.md", 20), ("y.md", 21), ("z.md", 22)]);
    // Two cached notes share inode 20; a scan collapses hard links, so two entries share 21 by hand.
    let mut entries: Vec<Entry> = vault.entries().cloned().collect();
    for entry in &mut entries {
        if entry.path.as_path() == Path::new("z.md") {
            entry.identity = inode(21);
        }
    }

    let seed = seed_over(decoded, entries.iter(), &vault);

    assert_eq!(seeded(&seed, &vault), BTreeMap::new());
    assert_eq!(seed.words.snapshot().total, 0);
}

#[test]
fn ghost_subtracts_length() {
    let decoded = cached(&[
        ("a.md", "one two two", 30, fine()),
        ("b.md", "three", 31, fine()),
    ]);
    let vault = scanned(&[("b.md", 31)]);

    let seed = seed(decoded, &vault);

    assert_eq!(
        seeded(&seed, &vault),
        BTreeMap::from([("b.md".to_owned(), note(&["three"], 31))])
    );
    assert_eq!(seed.words.snapshot().total, 1, "a's three words are gone");
}

#[test]
fn stamp_none_is_a_ghost() {
    let decoded = cached(&[
        ("a.md", "alpha", 40, early()),
        ("q.md", "quark", 41, fine()),
        ("r.md", "rho", 42, early()),
        ("t.md", "tau", 42, fine()),
    ]);
    // `a` holds its path but no stamp, and `q` moved onto it; `r` and `t` shared inode 42.
    let vault = scanned(&[("a.md", 41), ("s.md", 42)]);

    let seed = seed(decoded, &vault);

    let expected = BTreeMap::from([
        ("a.md".to_owned(), note(&["quark"], 41)),
        ("s.md".to_owned(), note(&["tau"], 42)),
    ]);
    assert_eq!(seeded(&seed, &vault), expected);
    assert_eq!(seed.words.snapshot().total, 2);
}

#[test]
fn a_whole_second_stamp_is_seeded_and_not_listed() {
    let whole = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(2));
    let decoded = cached(&[("a.md", "alpha", 50, whole), ("b.md", "beta", 51, fine())]);
    let vault = scanned(&[("a.md", 50), ("b.md", 51)]);

    let seed = seed(decoded, &vault);

    let (words, read, _) = note(&["alpha"], 50);
    let expected = BTreeMap::from([
        ("a.md".to_owned(), (words, read, false)),
        ("b.md".to_owned(), note(&["beta"], 51)),
    ]);
    assert_eq!(seeded(&seed, &vault), expected);
}

#[test]
fn two_paths_one_lossy_name_match_nothing_by_path() {
    let decoded = cached(&[("a\u{FFFD}.md", "alpha", 60, fine())]);
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for (byte, ino) in [(0xfe, 61), (0xff, 62)] {
        let name = [b'a', byte, b'.', b'm', b'd'];
        let path = Path::new(ROOT).join(OsStr::from_bytes(&name));
        fs.insert_file_with_identity(path, "", inode(ino));
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new(ROOT))) else {
        panic!("a MemFs scan cannot fail");
    };

    let seed = seed(decoded, &vault);

    assert_eq!(seeded(&seed, &vault), BTreeMap::new());
}
