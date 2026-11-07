//! `LiveIndex` driven by hand: each input, and each race between them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Stamp, Vault, VaultError};

use super::{BuildJob, Effects, LiveIndex, Load, Merge, Status, Ticket, Turn};
use crate::test_support::{ROOT, content, find_all, id, loaded, target};
use crate::words::{NoteWords, Words};
use crate::{FileRead, NoteContent, NoteQuery, Resolution};

/// Enough slots that no test below the slot tests waits for one.
pub(super) const SLOTS: usize = 64;

pub(super) struct Harness {
    pub(super) fs: MemFs,
    pub(super) vault: Vault,
    pub(super) live: LiveIndex,
    pub(super) pending: Pending,
}

/// The jobs `Harness::turn` kept, not run yet.
#[derive(Default)]
pub(super) struct Pending {
    pub(super) loads: Vec<Load>,
    pub(super) build: Option<BuildJob>,
    pub(super) merge: Option<Merge>,
}

impl Harness {
    /// A scanned vault holding `files` with their texts; nothing is built yet.
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
        Self::with_slots(files, SLOTS)
    }

    pub(super) fn with_slots(files: &[(&str, &str)], slots: usize) -> Self {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        for (file, text) in files {
            fs.insert_file(path(file), *text);
        }
        let Ok(vault) = block_on(Vault::scan(&fs, Path::new(ROOT))) else {
            panic!("a MemFs scan cannot fail");
        };
        Self {
            fs,
            vault,
            live: LiveIndex::new(slots),
            pending: Pending::default(),
        }
    }

    /// Built and every note loaded, each in its own turn; the settle's merge is pending.
    pub(super) fn loaded(files: &[(&str, &str)]) -> Self {
        Self::loaded_with_slots(files, SLOTS)
    }

    fn loaded_with_slots(files: &[(&str, &str)], slots: usize) -> Self {
        let mut harness = Self::with_slots(files, slots);
        let _ = harness.built();
        harness.load_all();
        harness
    }

    pub(super) fn rescan(&mut self) {
        let Ok(()) = block_on(self.vault.rescan(&self.fs)) else {
            panic!("a MemFs rescan cannot fail");
        };
    }

    /// Written outside the app, then rescanned.
    pub(super) fn write(&mut self, file: &str, text: &str) {
        self.fs.insert_file(path(file), text);
        self.rescan();
    }

    pub(super) fn remove(&mut self, file: &str) {
        self.fs.remove(&path(file));
        self.rescan();
    }

    /// The first pending load, taken to land by hand.
    fn start(&mut self) -> Load {
        assert!(!self.pending.loads.is_empty(), "a note is queued");
        self.pending.loads.remove(0)
    }

    pub(super) fn read(&self, load: &Load) -> inkra_vault::Result<NoteContent> {
        block_on(self.fs.load(&load.path)).map(|loaded| {
            let file = FileRead {
                stamp: loaded.stamp,
                identity: loaded.identity,
            };
            NoteContent::parse(&loaded.text, file)
        })
    }

    /// Lands `loaded` in its own turn; true when an answer moved.
    fn landed(&mut self, ticket: Ticket, loaded: inkra_vault::Result<NoteContent>) -> bool {
        let revision = self.live.revision();
        let _ = self.turn(|t| t.landed(ticket, loaded));
        self.live.revision() != revision
    }

    /// Reads the file now and lands the result.
    fn land(&mut self, load: &Load) -> bool {
        let loaded = self.read(load);
        self.landed(load.ticket, loaded)
    }

    fn fail(&mut self, ticket: Ticket) -> bool {
        self.landed(ticket, Err(failure()))
    }

    /// Lands every pending load, each in its own turn, until none is left.
    pub(super) fn load_all(&mut self) {
        while !self.pending.loads.is_empty() {
            let _ = self.land_pending(0);
        }
    }

    /// What `IndexSync::saved` does: the file is written, and the index takes the text it wrote.
    fn saving(&mut self, file: &str, text: &str) -> (Ticket, NoteContent) {
        self.fs.insert_file(path(file), text);
        let stamp = self.stamp_on_disk(file);
        let id = self.id(file);
        let (ticket, _) = self.turn(|t| t.saved(id, stamp));
        (ticket, content(text, stamp.mtime))
    }

    fn save(&mut self, file: &str, text: &str) {
        let (ticket, content) = self.saving(file, text);
        self.landed(ticket, Ok(content));
    }

    /// Read from the folder listing, so `MemFs::loads` counts no load.
    fn stamp_on_disk(&self, file: &str) -> Stamp {
        let path = path(file);
        let Some(dir) = path.parent() else {
            panic!("{file} is under the root");
        };
        let Ok(listed) = block_on(self.fs.read_dir(dir)) else {
            panic!("{file}'s folder is on disk");
        };
        let Some(entry) = listed.into_iter().find(|entry| entry.path == path) else {
            panic!("{file} is on disk");
        };
        Stamp {
            mtime: entry.mtime,
            size: entry.size,
        }
    }

    pub(super) fn id(&self, file: &str) -> inkra_vault::EntryId {
        id(&self.vault, file)
    }

    /// The size and words the index holds for `file`; `None` when it holds no stamp.
    fn held(&self, file: &str) -> Option<Held> {
        let id = self.id(file);
        let (_, stamp) = loaded(&self.live.index().snapshot())
            .into_iter()
            .find(|(held, _)| *held == id)?;
        Some((
            stamp.size,
            words_of(&self.live.index().words().entries(), id),
        ))
    }

    fn loads(&self, file: &str) -> usize {
        self.fs.loads(&path(file))
    }

    /// Runs one turn; its loads are copied to `pending`, its build and merge move there.
    pub(super) fn turn<R>(&mut self, inputs: impl FnOnce(&mut Turn<'_>) -> R) -> (R, Effects) {
        let (result, mut effects) = self.live.turn(&self.vault, inputs);
        let copies = effects.loads.iter().map(|load| Load {
            ticket: load.ticket,
            path: load.path.clone(),
        });
        self.pending.loads.extend(copies);
        if let Some(build) = effects.build.take() {
            assert!(
                self.pending.build.replace(build).is_none(),
                "one build runs (L6)"
            );
        }
        if let Some(merge) = effects.merge.take() {
            assert!(
                self.pending.merge.replace(merge).is_none(),
                "one merge runs"
            );
        }
        (result, effects)
    }

    /// A vault change in its own turn, then its build diffs the vault now and applies in another.
    pub(super) fn built(&mut self) -> Effects {
        assert!(self.pending.build.is_none(), "no build runs");
        let _ = self.vault_changed();
        self.run_build()
    }

    /// A vault change in its own turn.
    pub(super) fn vault_changed(&mut self) -> Effects {
        // Typed, or clippy asks for the path `Turn::vault_changed`, which binds one lifetime and fails the bound.
        self.turn(|t: &mut Turn<'_>| t.vault_changed()).1
    }

    /// Runs the pending build and applies it in its own turn.
    pub(super) fn run_build(&mut self) -> Effects {
        let Some(job) = self.pending.build.take() else {
            panic!("a build is pending");
        };
        let built = job.run();
        self.turn(|t| t.built(built)).1
    }

    /// Reads the pending load `at` now and lands it in its own turn.
    pub(super) fn land_pending(&mut self, at: usize) -> Effects {
        let load = self.pending.loads.remove(at);
        let loaded = self.read(&load);
        self.turn(|t| t.landed(load.ticket, loaded)).1
    }

    /// Runs the pending merge and applies it in its own turn.
    pub(super) fn run_merge(&mut self) -> Effects {
        let Some(merge) = self.pending.merge.take() else {
            panic!("a merge is pending");
        };
        self.merged(merge)
    }

    /// Runs `merge` and applies it in its own turn.
    pub(super) fn merged(&mut self, merge: Merge) -> Effects {
        let merged = merge.run();
        self.turn(|t| t.merged(merged)).1
    }

    /// A turn with no input.
    pub(super) fn idle_turn(&mut self) -> Effects {
        self.turn(|_| ()).1
    }
}

/// A note's size and its `(word, count)`s.
type Held = (u64, Vec<(String, u32)>);

/// What a load of `text` gives the index to hold.
fn parsed(text: &str) -> Held {
    let mut words = Words::default();
    words.set(EntryId::DETACHED, NoteWords::parse(text));
    let entries = words.snapshot().entries();
    (text.len() as u64, words_of(&entries, EntryId::DETACHED))
}

/// `id`'s `(word, count)`s among `entries`.
pub(super) fn words_of(
    entries: &BTreeMap<(String, EntryId), u32>,
    id: EntryId,
) -> Vec<(String, u32)> {
    entries
        .iter()
        .filter(|((_, note), _)| *note == id)
        .map(|((word, _), count)| (word.clone(), *count))
        .collect()
}

pub(super) fn path(file: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{file}"))
}

pub(super) fn paths(loads: &[Load]) -> Vec<PathBuf> {
    loads.iter().map(|load| load.path.clone()).collect()
}

pub(super) fn failure() -> VaultError {
    VaultError::NotUtf8 {
        path: PathBuf::from(ROOT),
    }
}

#[test]
fn a_build_asked_while_one_runs_is_dirty_and_runs_after_it() {
    let mut h = Harness::new(&[("a.md", ""), ("b.md", "")]);
    let _ = h.vault_changed();
    assert!(h.pending.build.is_some(), "the change starts a build");

    // `Harness::turn` refuses a second job while one is pending.
    let _ = h.vault_changed();
    let _ = h.vault_changed();
    let _ = h.run_build();
    assert!(
        h.pending.build.is_some(),
        "dirty: its apply asks for another"
    );
    let _ = h.run_build();
    assert!(h.pending.build.is_none(), "the rebuild saw no notify");
}

#[test]
fn a_build_landing_behind_the_vault_applies() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.write("c.md", "");
    let _ = h.vault_changed();
    h.write("d.md", "");
    let _ = h.vault_changed();

    let applied = h.run_build();

    assert!(h.pending.build.is_some(), "dirty");
    assert!(applied.layout, "c joined before the read");
    assert_eq!(h.live.index().names().len(), 3, "read before d");
    assert!(h.run_build().layout);
    assert_eq!(h.live.index().names().len(), 4);
}

#[test]
fn a_vault_with_no_notes_applies_one_layout_and_stops_building() {
    let mut h = Harness::new(&[("image.png", "")]);
    assert!(h.live.awaits_first_build());
    assert!(h.live.is_building());

    assert!(h.built().layout, "the first build");

    assert!(!h.live.awaits_first_build());
    assert!(!h.live.is_building());
    assert!(!h.built().layout, "empty against empty moves nothing");
}

#[test]
fn status_goes_scanning_indexing_ready() {
    let mut h = Harness::new(&[("a.md", ""), ("b.md", "")]);
    assert_eq!(h.live.status(), Status::Scanning);

    let _ = h.built();
    let (a, b) = (h.start(), h.start());
    assert_eq!(
        h.live.status(),
        Status::Indexing {
            loaded: 0,
            total: 2
        }
    );

    h.land(&a);
    assert_eq!(
        h.live.status(),
        Status::Indexing {
            loaded: 1,
            total: 2
        }
    );

    h.land(&b);
    assert_eq!(h.live.status(), Status::Ready);
}

#[test]
fn an_mtime_only_change_queues_that_note_and_keeps_the_set() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", ""), ("c.md", "")]);
    h.write("a.md", "new");

    assert!(!h.built().layout);

    let load = h.start();
    assert_eq!(load.path, path("a.md"));
    assert!(h.pending.loads.is_empty(), "only a moved");
    assert!(h.land(&load));
    assert_eq!(h.held("a.md"), Some(parsed("new")));
}

#[test]
fn a_save_since_the_build_read_is_not_queued() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let _ = h.vault_changed();
    h.save("a.md", "saved");

    let _ = h.run_build();

    assert!(
        h.pending.loads.is_empty(),
        "the save is newer than the read"
    );
    assert_eq!(h.held("a.md"), Some(parsed("saved")));
}

#[test]
fn a_rescan_finding_the_saved_mtime_loads_nothing() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.save("a.md", "saved");
    h.rescan();

    let _ = h.built();

    assert!(h.pending.loads.is_empty());
    assert_eq!(h.loads("a.md"), 1);
}

#[test]
fn a_load_landing_after_a_newer_save_is_dropped() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let _ = h.built();
    let load = h.start();
    let read = h.read(&load);

    h.save("a.md", "saved");

    assert!(!h.landed(load.ticket, read));
    assert_eq!(h.held("a.md"), Some(parsed("saved")));
}

#[test]
fn a_stale_failure_keeps_the_newer_mtime() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let _ = h.built();
    let load = h.start();
    h.save("a.md", "saved");

    assert!(!h.fail(load.ticket));

    assert_eq!(h.held("a.md"), Some(parsed("saved")));
    h.rescan();
    let _ = h.built();
    assert!(
        h.pending.loads.is_empty(),
        "the save's mtime is still the one indexed"
    );
}

#[test]
fn a_note_queued_twice_is_loaded_once() {
    let mut h = Harness::loaded_with_slots(&[("a.md", ""), ("b.md", ""), ("c.md", "")], 1);
    h.fs.insert_file(path("a.md"), "a");
    h.write("b.md", "[[a]]");
    let _ = h.built();
    let a = h.start();
    assert_eq!(a.path, path("a.md"), "a is first in vault order");
    h.write("b.md", "[[c]]");
    let _ = h.built();

    assert!(h.pending.loads.is_empty(), "a holds the one slot");
    h.land(&a);
    h.load_all();

    assert_eq!(
        h.loads("b.md"),
        2,
        "the first queue entry for b was superseded"
    );
    assert_eq!(h.held("b.md"), Some(parsed("[[c]]")));
}

#[test]
fn a_note_queued_while_it_loads_waits_for_its_load() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.write("a.md", "[[c]]");
    let _ = h.built();
    let first = h.start();
    h.write("a.md", "[[b]]");
    let _ = h.built();

    assert!(h.pending.loads.is_empty(), "one read per note at a time");
    assert_eq!(
        h.live.progress(),
        (1, 2),
        "a is loading and queued, and counts once"
    );

    let stale = h.read(&first);
    h.landed(first.ticket, stale);
    let second = h.start();
    assert_eq!(second.path, path("a.md"));
    h.land(&second);
    assert_eq!(h.live.progress(), (2, 2));
    assert_eq!(h.held("a.md"), Some(parsed("[[b]]")));
}

#[test]
fn a_save_landing_during_a_load_keeps_its_slot() {
    let mut h = Harness::loaded_with_slots(&[("a.md", ""), ("b.md", "")], 1);
    h.fs.insert_file(path("a.md"), "outside");
    h.write("b.md", "outside");
    let _ = h.built();
    let a = h.start();
    assert_eq!(a.path, path("a.md"), "a is first in vault order");

    h.save("a.md", "saved");

    assert!(h.pending.loads.is_empty(), "a's load still holds the slot");
}

#[test]
fn a_load_in_flight_for_a_note_gone_is_not_pending() {
    let mut h = Harness::loaded_with_slots(&[("a.md", ""), ("b.md", ""), ("c.md", "")], 1);
    h.fs.insert_file(path("a.md"), "changed");
    h.write("b.md", "changed");
    let _ = h.built();
    let load = h.start();
    assert_eq!(load.path, path("a.md"), "a is first in vault order");
    assert_eq!(h.live.progress(), (1, 3), "a is loading, b is queued");

    h.fs.remove(&path("a.md"));
    h.remove("b.md");
    let _ = h.built();

    assert_eq!(h.live.progress(), (1, 1), "c, the only note, is loaded");
    assert!(!h.live.is_building());
    h.land(&load);
    assert_eq!(h.live.progress(), (1, 1));
}

#[test]
fn a_note_reloading_during_its_load_counts_once() {
    let mut h = Harness::loaded_with_slots(&[("a.md", ""), ("b.md", "")], 1);
    h.fs.insert_file(path("a.md"), "[[c]]");
    h.write("c.md", "");
    let _ = h.built();
    let first = h.start();
    assert_eq!(first.path, path("a.md"), "a is first in vault order");
    assert_eq!(
        h.live.progress(),
        (1, 3),
        "a is loading, c is queued behind it"
    );

    h.fs.insert_file(path("a.md"), "[[b]]");
    h.write("d.md", "");
    let _ = h.built();
    assert_eq!(
        h.live.progress(),
        (1, 4),
        "a is loading and queued again, c and d are queued"
    );

    h.land(&first);
    h.load_all();
    assert_eq!(h.live.progress(), (4, 4));
    assert_eq!(h.held("a.md"), Some(parsed("[[b]]")));
}

#[test]
fn a_load_failing_after_a_layout_is_retried() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.built();
    let load = h.start();
    h.write("c.md", "");
    assert!(h.built().layout);
    let c = h.start();
    h.land(&c);

    assert!(!h.fail(load.ticket), "a retry changes nothing yet");

    let retry = h.start();
    assert_eq!(retry.path, path("a.md"));
    h.land(&retry);
    assert_eq!(h.held("a.md"), Some(parsed("new")));
}

#[test]
fn a_load_failing_after_only_mtimes_forgets_the_note() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.built();
    let load = h.start();
    h.write("b.md", "moved");
    assert!(!h.built().layout);
    h.load_all();

    assert!(h.fail(load.ticket));

    assert_eq!(h.held("a.md"), None, "its stamp is dropped");
    assert!(!h.live.index().is_indexed(h.id("a.md")));
    assert!(
        h.pending.loads.is_empty(),
        "not retried before the next apply"
    );
}

#[test]
fn a_forgotten_note_is_queued_at_the_next_apply_of_either_kind() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.built();
    let load = h.start();
    h.write("b.md", "moved");
    let _ = h.built();
    h.load_all();
    assert!(h.fail(load.ticket));

    h.write("b.md", "again");
    assert!(!h.built().layout);
    h.load_all();
    assert_eq!(
        h.held("a.md"),
        Some(parsed("new")),
        "an mtime apply retries it"
    );

    let load = {
        h.write("a.md", "newer");
        let _ = h.built();
        h.start()
    };
    h.write("b.md", "once more");
    let _ = h.built();
    h.load_all();
    assert!(h.fail(load.ticket));
    h.write("c.md", "");
    assert!(h.built().layout);
    h.load_all();
    assert_eq!(
        h.held("a.md"),
        Some(parsed("newer")),
        "a layout apply retries it"
    );
}

#[test]
fn a_failed_load_behind_a_save_still_parsing_keeps_the_note() {
    let mut h = Harness::loaded(&[("a.md", "old words"), ("b.md", "")]);
    h.write("a.md", "changed");
    h.write("c.md", "");
    let _ = h.built();
    let load = h.start();
    let c = h.start();
    h.land(&c);
    let (_parsing, _) = h.saving("a.md", "saved");
    // The vault still lists the note: the batch that drops it has not landed.
    h.fs.remove(&path("a.md"));

    assert!(!h.fail(load.ticket));

    assert_eq!(h.held("a.md"), Some(parsed("old words")));
    assert!(h.pending.loads.is_empty(), "the save brings the text");
}

#[test]
fn a_failed_load_behind_a_queued_reload_keeps_the_note() {
    let mut h = Harness::loaded(&[("a.md", "[[b]] old"), ("b.md", "")]);
    h.write("a.md", "first");
    h.write("c.md", "");
    let _ = h.built();
    let first = h.start();
    h.write("a.md", "second");
    h.write("d.md", "");
    let _ = h.built();
    // The vault still lists the note: the batch that drops it has not landed.
    h.fs.remove(&path("a.md"));

    assert!(!h.fail(first.ticket));

    assert_eq!(h.held("a.md"), Some(parsed("[[b]] old")));
    assert!(
        paths(&h.pending.loads).contains(&path("a.md")),
        "the queued reload still runs"
    );
}

#[test]
fn a_save_landing_before_its_note_joins_is_reloaded_by_the_next_build() {
    let mut h = Harness::new(&[("a.md", "[[b]]"), ("b.md", "")]);
    let a = h.id("a.md");
    let stamp = h.stamp_on_disk("a.md");
    let (ticket, _) = h.turn(|t| t.saved(a, stamp));

    h.landed(ticket, Ok(content("[[b]]", stamp.mtime)));
    let _ = h.built();
    h.load_all();

    assert_eq!(h.held("a.md"), Some(parsed("[[b]]")));
    assert_eq!(h.loads("a.md"), 1, "the build reloads it");
}

#[test]
fn a_layout_moves_the_revision_once() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    let before = h.live.revision();
    h.write("a.md", "moved");
    assert!(!h.built().layout);
    assert_eq!(h.live.revision(), before, "an mtime apply only queues");

    h.write("c.md", "");
    assert!(h.built().layout);

    assert_eq!(h.live.revision(), before + 1, "one layout, no load");
}

#[test]
fn a_failed_load_moves_the_revision() {
    let mut h = Harness::loaded(&[("a.md", "[[b]]"), ("b.md", "")]);
    h.write("a.md", "changed");
    let _ = h.built();
    let load = h.start();
    let before = h.live.revision();

    assert!(h.fail(load.ticket));

    assert_eq!(h.live.revision(), before + 1, "a's forget");
}

#[test]
fn a_stale_ticket_a_merge_and_a_mint_leave_the_revision() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    let _ = h.built();
    h.load_all();
    assert!(
        h.pending.merge.is_some(),
        "the load's end starts the settle"
    );
    let before = h.live.revision();
    let _ = h.run_merge();
    assert_eq!(h.live.revision(), before, "a merge");

    h.write("a.md", "outside");
    let _ = h.built();
    let load = h.start();
    let before = h.live.revision();
    let (ticket, content) = h.saving("a.md", "saved");
    assert_eq!(h.live.revision(), before, "a mint");
    assert!(!h.land(&load));
    assert_eq!(h.live.revision(), before, "a stale ticket");

    assert!(h.landed(ticket, Ok(content)));
    assert_eq!(h.live.revision(), before + 1, "the save's parse");
}

#[test]
fn an_asset_gives_no_name_no_word_and_no_load() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("photo.png", "png")]);
    let mut effects = vec![h.built()];
    while !h.pending.loads.is_empty() {
        effects.push(h.land_pending(0));
    }

    let photo = path("photo.png");
    assert!(
        effects
            .iter()
            .flat_map(|effects| &effects.loads)
            .all(|load| load.path != photo)
    );
    assert_eq!(h.loads("photo.png"), 0);
    let snapshot = h.live.index().snapshot();
    for query in ["photo", "png"] {
        assert!(find_all(&snapshot, &NoteQuery::new(query, None, &[]), 10).is_empty());
    }
    assert_eq!(
        find_all(&snapshot, &NoteQuery::new("alpha", None, &[]), 10).len(),
        1
    );
}

#[test]
fn an_asset_written_loads_nothing_and_keeps_the_set() {
    let mut h = Harness::loaded(&[("a.md", "alpha"), ("photo.png", "png")]);
    let before = h.live.revision();

    h.write("photo.png", "png, edited");
    assert!(!h.built().layout, "a moved asset stamp is no note's");
    assert!(h.pending.loads.is_empty());
    assert_eq!(h.live.revision(), before);
}

#[test]
fn adding_an_asset_rebuilds_the_layout() {
    let mut h = Harness::loaded(&[("a.md", "![[x.png]]")]);
    let a = h.id("a.md");

    h.fs.insert_bytes(path("x.png"), *b"\x89PNG");
    h.rescan();
    let _ = h.vault_changed();
    let effects = h.run_build();

    assert!(effects.layout, "links resolve to the new asset");
    assert!(h.pending.loads.is_empty());
    assert_eq!(
        h.live.index().resolve(Some(a), &target("![[x.png]]")),
        Resolution::File(h.id("x.png"))
    );
}

#[test]
fn removing_or_renaming_an_asset_rebuilds_the_layout() {
    let mut h = Harness::loaded(&[("a.md", "alpha"), ("x.png", "png"), ("y.png", "png")]);

    let renamed = block_on(h.fs.rename(&path("x.png"), &path("z.png")));
    assert!(renamed.is_ok());
    h.rescan();
    assert!(h.built().layout, "renamed");
    assert!(h.pending.loads.is_empty());

    h.remove("y.png");
    assert!(h.built().layout, "removed");
    assert!(h.pending.loads.is_empty());
}
