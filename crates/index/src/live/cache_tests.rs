//! A relaunch over the persisted index: what the seed reuses, what it reloads (persisted index spec, P5, P8).

use std::collections::BTreeMap;
use std::mem;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Kind, Mtime, Stamp, Vault};

use super::tests::{Harness, Pending, SLOTS, path, paths};
use super::{Effects, LiveIndex, Status};
use crate::persist::{Outcome, Rejected, decode, encode};
use crate::test_support::{ROOT, find_all, loaded as stamps};
use crate::words::FRESH_MIN;
use crate::{FileRead, NoteContent, NoteIndex, NoteQuery};

/// `n` seconds and a half after the epoch: a sub-second mtime, as a real disk writes.
fn fine(n: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(n) + Duration::from_millis(500))
}

fn scan(fs: &MemFs) -> Vault {
    let Ok(vault) = block_on(Vault::scan(fs, Path::new(ROOT))) else {
        panic!("a MemFs scan cannot fail");
    };
    vault
}

/// Built and loaded over `files`, each written at its mtime.
fn loaded_at(files: &[(&str, &str, Mtime)]) -> Harness {
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for &(file, text, mtime) in files {
        fs.insert_with_mtime(path(file), text, mtime);
    }
    let mut h = Harness {
        vault: scan(&fs),
        fs,
        live: LiveIndex::new(SLOTS),
        pending: Pending::default(),
    };
    let _ = h.built();
    h.load_all();
    h
}

/// Built and loaded over `files`, the nth written at `fine(n + 1)`.
fn loaded(files: &[(&str, &str)]) -> Harness {
    let files: Vec<_> = (1..)
        .zip(files)
        .map(|(n, &(file, text))| (file, text, fine(n)))
        .collect();
    loaded_at(&files)
}

/// `index` as the cache file gives it back.
fn cache_of(index: &NoteIndex) -> Outcome {
    match decode(&encode(&index.snapshot(), Path::new(ROOT)), Path::new(ROOT)) {
        Ok(decoded) => Outcome::Decoded(decoded),
        Err(rejected) => panic!("rejected: {}", rejected.reason()),
    }
}

/// A new launch over `fs`, waiting for the cache; nothing is built yet.
fn launch(fs: MemFs) -> Harness {
    let mut live = LiveIndex::new(SLOTS);
    live.await_cache();
    Harness {
        vault: scan(&fs),
        fs,
        live,
        pending: Pending::default(),
    }
}

/// `h`'s cache, then a new launch over the same disk.
fn relaunch(h: Harness) -> (Harness, Outcome) {
    let outcome = cache_of(h.live.index());
    (launch(h.fs), outcome)
}

/// The outcome in its own turn, then the build it starts over the scanned vault.
fn seeded(h: &mut Harness, outcome: Outcome) -> Effects {
    let _ = h.turn(|t| t.cached(outcome));
    h.run_build()
}

/// A save of `text` at `mtime`, landed; the vault holds its stamp, as `NoteStore`'s re-key does.
fn save(h: &mut Harness, file: &str, text: &str, mtime: Mtime) -> Stamp {
    h.fs.insert_with_mtime(path(file), text, mtime);
    h.rescan();
    let id = h.id(file);
    let Some(entry) = h.vault.entry(id) else {
        panic!("{file} is listed");
    };
    let read = FileRead {
        stamp: entry.stamp(),
        identity: entry.identity,
    };
    let (ticket, _) = h.turn(|t| t.saved(id, read.stamp));
    let _ = h.turn(|t| t.landed(ticket, Ok(NoteContent::parse(text, read))));
    read.stamp
}

fn file_of(h: &Harness, id: inkra_vault::EntryId) -> String {
    let Some(entry) = h.vault.entry(id) else {
        panic!("{id:?} is listed");
    };
    entry.path.as_path().to_string_lossy().into_owned()
}

/// The words the index holds for `file`.
fn words(h: &Harness, file: &str) -> Vec<String> {
    let id = h.id(file);
    let entries = h.live.index().words().entries();
    entries
        .keys()
        .filter(|(_, note)| *note == id)
        .map(|(word, _)| word.clone())
        .collect()
}

/// The notes linking to `file`.
fn backlinks(h: &Harness, file: &str) -> Vec<String> {
    let index = h.live.index();
    let mut sources: Vec<String> = index
        .backlinks(h.id(file))
        .iter()
        .map(|backlink| file_of(h, backlink.source))
        .collect();
    sources.sort();
    sources
}

/// What a reader sees, by path: the words, the stamps, the backlinks.
type View = (
    BTreeMap<(String, String), u32>,
    Vec<(String, Stamp)>,
    Vec<(String, Vec<String>)>,
);

fn view(h: &Harness) -> View {
    let index = h.live.index();
    let words = index
        .words()
        .entries()
        .into_iter()
        .map(|((word, id), count)| ((word, file_of(h, id)), count))
        .collect();
    let stamps = stamps(&index.snapshot())
        .into_iter()
        .map(|(id, stamp)| (file_of(h, id), stamp))
        .collect();
    let links = h
        .vault
        .entries()
        .filter(|entry| entry.kind == Kind::Note)
        .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
        .map(|file| {
            let sources = backlinks(h, &file);
            (file, sources)
        })
        .collect();
    (words, stamps, links)
}

/// `h`'s disk built cold under the same ids, every note loaded.
fn cold(h: &mut Harness) -> Harness {
    let mut cold = Harness {
        fs: mem::take(&mut h.fs),
        vault: h.vault.clone(),
        live: LiveIndex::new(SLOTS),
        pending: Pending::default(),
    };
    let _ = cold.built();
    cold.load_all();
    cold
}

fn loading(h: &Harness) -> Vec<PathBuf> {
    paths(&h.pending.loads)
}

#[test]
fn size_only_change_reloads() {
    let mut h = loaded(&[("a.md", "one"), ("b.md", "")]);
    h.fs.insert_with_mtime(path("a.md"), "three", fine(1));
    h.rescan();

    let _ = h.built();

    assert_eq!(
        loading(&h),
        [path("a.md")],
        "a's size moved, its mtime did not"
    );
}

#[test]
fn unchanged_notes_load_nothing() {
    let (mut h, outcome) = relaunch(loaded(&[("a.md", "alpha [[b]]"), ("b.md", "beta")]));

    let effects = seeded(&mut h, outcome);

    assert_eq!(loading(&h), Vec::<PathBuf>::new());
    assert_eq!(h.fs.loads(&path("a.md")), 1, "the first launch's load only");
    assert_eq!(h.live.status(), Status::Ready);
    assert!(effects.indexed, "the seed's turn is the startup's last");
    let cold = cold(&mut h);
    assert_eq!(view(&h), view(&cold));
}

#[test]
fn whole_second_stamp_reloads() {
    let whole = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(7));
    let (mut h, outcome) = relaunch(loaded_at(&[
        ("a.md", "alpha", whole),
        ("b.md", "beta", fine(8)),
    ]));

    let _ = seeded(&mut h, outcome);

    // P8: a write in a's tick after its load would leave the same stamp; b's equal stamp is reused.
    assert_eq!(loading(&h), [path("a.md")]);
}

#[test]
fn ghost_ordinal_dropped() {
    let h = loaded(&[
        ("a.md", "plan plan plan"),
        ("b.md", "plan"),
        ("c.md", "plan alpha beta"),
    ]);
    // Gone from disk, still in the index the cache is written from.
    h.fs.remove(&path("a.md"));
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(loading(&h), Vec::<PathBuf>::new());
    let cold = cold(&mut h);
    let query = NoteQuery::new("plan", None, &[]);
    let found = find_all(&h.live.index().snapshot(), &query, 10);
    assert_eq!(found.len(), 2);
    assert_eq!(found, find_all(&cold.live.index().snapshot(), &query, 10));
}

#[test]
fn save_before_seed_keeps_its_stamp_and_words() {
    let (mut h, outcome) = relaunch(loaded(&[("a.md", "old"), ("b.md", "")]));
    let _ = h.built();
    let stamp = save(&mut h, "a.md", "new", fine(10));

    let _ = seeded(&mut h, outcome);

    assert_eq!(h.live.indexed.get(&h.id("a.md")), Some(&stamp));
    assert_eq!(words(&h, "a.md"), ["new"]);
    assert_eq!(loading(&h), Vec::<PathBuf>::new(), "the save is the file");
    let _ = h.built();
    assert_eq!(loading(&h), Vec::<PathBuf>::new(), "a rescan loads nothing");
    let cold = cold(&mut h);
    assert_eq!(view(&h), view(&cold));
}

#[test]
fn save_landed_before_first_build_reloads() {
    let (mut h, outcome) = relaunch(loaded(&[("a.md", "old"), ("b.md", "")]));
    let _ = save(&mut h, "a.md", "new", fine(10));

    let _ = seeded(&mut h, outcome);

    assert_eq!(loading(&h), [path("a.md")], "the save landed on no note");
    h.load_all();
    let cold = cold(&mut h);
    assert_eq!(view(&h), view(&cold));
}

#[test]
fn rename_with_edit_reloads_under_new_path() {
    let h = loaded(&[("a.md", "alpha"), ("b.md", "beta")]);
    let renamed = block_on(h.fs.rename(&path("a.md"), &path("z.md")));
    assert!(renamed.is_ok());
    h.fs.insert_with_mtime(path("z.md"), "zeta", fine(10));
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(words(&h, "z.md"), ["alpha"], "seeded by its inode");
    assert_eq!(loading(&h), [path("z.md")]);
    h.load_all();
    let cold = cold(&mut h);
    assert_eq!(view(&h), view(&cold));
}

#[test]
fn reused_inode_other_stamp_reloads() {
    let h = loaded(&[("a.md", "alpha")]);
    let Some(gone) = h.vault.entry(h.id("a.md")) else {
        panic!("a is listed");
    };
    h.fs.remove(&path("a.md"));
    h.fs.insert_file_with_identity(path("b.md"), "omega", gone.identity);
    h.fs.insert_with_mtime(path("b.md"), "omega", fine(10));
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(words(&h, "b.md"), ["alpha"], "seeded by its inode");
    assert_eq!(loading(&h), [path("b.md")], "same size, another mtime");
    h.load_all();
    let cold = cold(&mut h);
    assert_eq!(view(&h), view(&cold));
}

#[test]
fn by_stem_follows_rename() {
    let h = loaded(&[("a.md", "[[b]]"), ("b.md", ""), ("c.md", "")]);
    let renamed = block_on(h.fs.rename(&path("a.md"), &path("z.md")));
    assert!(renamed.is_ok());
    h.fs.insert_with_mtime(path("z.md"), "[[c]]", fine(10));
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(backlinks(&h, "b.md"), ["z.md"], "seeded under its new id");
    h.load_all();
    assert_eq!(backlinks(&h, "c.md"), ["z.md"]);
    assert_eq!(backlinks(&h, "b.md"), Vec::<String>::new());
}

#[test]
fn wait_blocks_queue_and_done() {
    let h = loaded(&[("a.md", "alpha"), ("b.md", "")]);
    h.fs.insert_with_mtime(path("a.md"), "changed", fine(10));
    let (mut h, _) = relaunch(h);

    let effects = h.built();

    assert_eq!(
        loading(&h),
        Vec::<PathBuf>::new(),
        "no load before the outcome"
    );
    assert!(!effects.indexed);
    assert_eq!(
        h.live.status(),
        Status::Indexing {
            loaded: 2,
            total: 3
        }
    );
    let effects = h.idle_turn();
    assert!(!effects.indexed);
    assert!(h.pending.build.is_none());

    let _ = h.turn(|t| t.cached(Outcome::Missing));
    assert!(
        h.pending.build.is_some(),
        "a build applied, so the outcome starts one"
    );
    let _ = h.run_build();

    assert_eq!(loading(&h), [path("a.md"), path("b.md")]);
    h.load_all();
    let effects = h.run_merge();
    assert!(effects.indexed, "the settle ends the startup");
}

#[test]
fn an_empty_vault_waits_as_one_note() {
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    let mut h = launch(fs);

    let effects = h.built();

    assert_eq!(
        h.live.status(),
        Status::Indexing {
            loaded: 0,
            total: 1
        }
    );
    assert!(!effects.indexed);
    let _ = h.turn(|t| t.cached(Outcome::Missing));
    let effects = h.run_build();
    assert_eq!(h.live.status(), Status::Ready);
    assert!(effects.indexed);
}

#[test]
fn an_outcome_during_a_build_rides_the_next() {
    let h = loaded(&[("a.md", "alpha"), ("b.md", "beta")]);
    h.fs.insert_with_mtime(path("a.md"), "changed", fine(10));
    let (mut h, outcome) = relaunch(h);
    let _ = h.vault_changed();

    let _ = h.turn(|t| t.cached(outcome));
    let _ = h.run_build();

    assert_eq!(
        loading(&h),
        Vec::<PathBuf>::new(),
        "that build read before the outcome"
    );
    let _ = h.run_build();
    assert_eq!(loading(&h), [path("a.md")]);
}

#[test]
fn no_merge_starts_while_waiting() {
    let files: Vec<String> = (0..FRESH_MIN).map(|n| format!("n{n:03}.md")).collect();
    let fs = MemFs::new();
    fs.insert_dir(ROOT);
    for file in &files {
        fs.insert_with_mtime(path(file), "", fine(1));
    }
    let mut h = launch(fs);
    let _ = h.built();
    for (n, file) in (2..).zip(&files) {
        let _ = save(&mut h, file, "word", fine(n));
    }
    assert!(
        h.pending.merge.is_none(),
        "{FRESH_MIN} fresh notes wait for the seed"
    );

    let _ = seeded(&mut h, cache_of(&NoteIndex::default()));

    assert_eq!(loading(&h), Vec::<PathBuf>::new());
    assert_eq!(words(&h, &files[0]), ["word"], "the saves outlive the seed");
    assert!(h.pending.merge.is_some(), "the wait over, the merge is due");
}

#[test]
#[should_panic(expected = "a cache outcome lands once, after `await_cache`")]
fn an_outcome_nobody_awaits_panics() {
    let mut h = Harness::new(&[("a.md", "")]);
    let _ = h.turn(|t| t.cached(Outcome::Missing));
}

#[test]
fn only_the_build_carrying_a_decoded_cache_is_seeded() {
    let (mut h, outcome) = relaunch(loaded(&[("a.md", "alpha"), ("b.md", "")]));
    assert!(!h.built().seeded, "this build read before the outcome");

    let ((), landing) = h.turn(|t| t.cached(outcome));
    let effects = h.run_build();

    assert!(
        !landing.seeded,
        "the outcome's own turn only starts the build"
    );
    assert!(effects.seeded);
    h.fs.insert_with_mtime(path("c.md"), "", fine(10));
    h.rescan();
    assert!(!h.built().seeded, "a later rescan's build carries no cache");
}

#[test]
fn a_missing_cache_seeds_nothing() {
    let (mut h, _) = relaunch(loaded(&[("a.md", "alpha")]));

    let effects = seeded(&mut h, Outcome::Missing);

    assert!(!effects.seeded);
}

#[test]
fn each_outcome_has_its_name() {
    assert_eq!(cache_of(&NoteIndex::default()).name(), "decoded");
    assert_eq!(Outcome::Rejected(Rejected::Short).name(), "rejected");
    assert_eq!(Outcome::Missing.name(), "missing");
}

#[test]
fn a_reload_held_at_encode_writes_the_stamp_of_the_words() {
    let mut h = loaded(&[("a.md", "alpha")]);
    h.fs.insert_with_mtime(path("a.md"), "bravo", fine(2));
    h.rescan();
    let _ = h.built();
    assert_eq!(loading(&h), [path("a.md")], "queued, not landed");
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(loading(&h), [path("a.md")], "the file holds alpha's stamp");
    h.load_all();
    let snapshot = h.live.index().snapshot();
    let bravo = find_all(&snapshot, &NoteQuery::new("bravo", None, &[]), 10);
    assert_eq!(bravo.len(), 1);
    assert!(find_all(&snapshot, &NoteQuery::new("alpha", None, &[]), 10).is_empty());
}

#[test]
fn a_fresh_reload_survives_a_relaunch() {
    let mut h = loaded(&[("a.md", "alpha")]);
    let _ = h.run_merge();
    let _ = save(&mut h, "a.md", "zulu", fine(3));
    assert!(!h.live.index().words_settled(), "zulu waits in fresh");
    let (mut h, outcome) = relaunch(h);

    let _ = seeded(&mut h, outcome);

    assert_eq!(loading(&h), Vec::<PathBuf>::new());
    let zulu = find_all(
        &h.live.index().snapshot(),
        &NoteQuery::new("zulu", None, &[]),
        10,
    );
    assert_eq!(zulu.len(), 1);
}
