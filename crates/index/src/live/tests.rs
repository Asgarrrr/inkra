//! `LiveIndex` driven by hand: each input, and each race between them.

use std::iter;
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Mtime, Vault, VaultError};

use super::{Applied, LiveIndex, Load, Ticket, TurnEnd};
use crate::NoteContent;
use crate::test_support::{ROOT, id, texts};

/// Enough slots that no test below the slot tests waits for one.
const SLOTS: usize = 64;

pub(super) struct Harness {
    pub(super) fs: MemFs,
    pub(super) vault: Vault,
    pub(super) live: LiveIndex,
}

impl Harness {
    /// A scanned vault holding `files` with their texts; nothing is built yet.
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
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
            live: LiveIndex::default(),
        }
    }

    /// Built and every note loaded.
    fn loaded(files: &[(&str, &str)]) -> Self {
        let mut harness = Self::new(files);
        harness.build();
        harness.load_all();
        harness
    }

    pub(super) fn rescan(&mut self) {
        let Ok(()) = block_on(self.vault.rescan(&self.fs)) else {
            panic!("a MemFs rescan cannot fail");
        };
    }

    /// Written outside the app, then rescanned.
    fn write(&mut self, file: &str, text: &str) {
        self.fs.insert_file(path(file), text);
        self.rescan();
    }

    fn remove(&mut self, file: &str) {
        self.fs.remove(&path(file));
        self.rescan();
    }

    pub(super) fn build(&mut self) -> Applied {
        let Some(job) = self.live.start_build(self.vault.shared_entries()) else {
            panic!("no build runs");
        };
        self.live.apply_build(job.run(), &self.vault)
    }

    pub(super) fn next(&mut self, slots: usize) -> Option<Load> {
        self.live.next_load(&self.vault, slots)
    }

    fn start(&mut self) -> Load {
        let Some(load) = self.next(SLOTS) else {
            panic!("a note is queued");
        };
        load
    }

    pub(super) fn read(&self, load: &Load) -> inkra_vault::Result<NoteContent> {
        block_on(self.fs.load(&load.path))
            .map(|loaded| NoteContent::parse(loaded.text.into(), loaded.stamp.mtime))
    }

    /// Reads the file now and lands the result.
    pub(super) fn land(&mut self, load: &Load) -> bool {
        let loaded = self.read(load);
        self.live.landed(load.ticket, loaded, &self.vault)
    }

    fn fail(&mut self, ticket: Ticket) -> bool {
        self.live.landed(ticket, Err(failure()), &self.vault)
    }

    pub(super) fn load_all(&mut self) {
        while let Some(load) = self.next(SLOTS) {
            self.land(&load);
        }
    }

    /// What `IndexSync::saved` does: the file is written, and the index takes the text it wrote.
    fn saving(&mut self, file: &str, text: &str) -> (Ticket, NoteContent) {
        self.fs.insert_file(path(file), text);
        let mtime = self.mtime_on_disk(file);
        let ticket = self.live.saved(self.id(file), mtime);
        (ticket, NoteContent::parse(text.into(), mtime))
    }

    fn save(&mut self, file: &str, text: &str) {
        let (ticket, content) = self.saving(file, text);
        self.live.landed(ticket, Ok(content), &self.vault);
    }

    /// Read from the folder listing, so `MemFs::loads` counts no load.
    fn mtime_on_disk(&self, file: &str) -> Mtime {
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
        entry.mtime
    }

    pub(super) fn id(&self, file: &str) -> inkra_vault::EntryId {
        id(&self.vault, file)
    }

    /// The text the index holds for `file`; `None` when it holds none.
    fn text(&self, file: &str) -> Option<String> {
        let id = self.id(file);
        texts(&self.live.index().corpus())
            .into_iter()
            .find(|(held, _)| *held == id)
            .map(|(_, text)| text)
    }

    fn loads(&self, file: &str) -> usize {
        self.fs.loads(&path(file))
    }

    pub(super) fn turn(&mut self) -> TurnEnd {
        self.live.end_turn()
    }
}

pub(super) fn path(file: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{file}"))
}

pub(super) fn failure() -> VaultError {
    VaultError::NotUtf8 {
        path: PathBuf::from(ROOT),
    }
}

#[test]
fn a_build_asked_while_one_runs_is_dirty_and_runs_after_it() {
    let mut h = Harness::new(&[("a.md", ""), ("b.md", "")]);
    let entries = h.vault.shared_entries();
    let Some(job) = h.live.start_build(entries.clone()) else {
        panic!("idle");
    };

    assert!(h.live.start_build(entries.clone()).is_none(), "one build");
    assert!(h.live.start_build(entries).is_none(), "still one");
    assert!(h.live.apply_build(job.run(), &h.vault).dirty);
    assert!(!h.build().dirty, "the rebuild saw no notify");
}

#[test]
fn a_build_landing_behind_the_vault_applies() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.write("c.md", "");
    let Some(job) = h.live.start_build(h.vault.shared_entries()) else {
        panic!("idle");
    };
    h.write("d.md", "");
    assert!(h.live.start_build(h.vault.shared_entries()).is_none());

    let applied = h.live.apply_build(job.run(), &h.vault);

    assert!(applied.dirty);
    assert!(applied.layout, "c joined before the read");
    drop(applied.retired);
    assert_eq!(h.live.index().names().len(), 3, "read before d");
    assert!(h.build().layout);
    assert_eq!(h.live.index().names().len(), 4);
}

#[test]
fn a_vault_with_no_notes_applies_one_layout_and_stops_building() {
    let mut h = Harness::new(&[("image.png", "")]);
    assert!(h.live.awaits_first_build());
    assert!(h.live.is_building());

    assert!(h.build().layout, "the first build");

    assert!(!h.live.awaits_first_build());
    assert!(!h.live.is_building());
    assert!(!h.build().layout, "empty against empty moves nothing");
}

#[test]
fn an_mtime_only_change_queues_that_note_and_keeps_the_set() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", ""), ("c.md", "")]);
    h.write("a.md", "new");

    assert!(!h.build().layout);

    let load = h.start();
    assert_eq!(load.path, path("a.md"));
    assert!(h.next(SLOTS).is_none(), "only a moved");
    assert!(h.land(&load));
    assert_eq!(h.text("a.md").as_deref(), Some("new"));
}

#[test]
fn a_save_since_the_build_read_is_not_queued() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let Some(job) = h.live.start_build(h.vault.shared_entries()) else {
        panic!("idle");
    };
    h.save("a.md", "saved");

    let _ = h.live.apply_build(job.run(), &h.vault);

    assert!(h.next(SLOTS).is_none(), "the save is newer than the read");
    assert_eq!(h.text("a.md").as_deref(), Some("saved"));
}

#[test]
fn a_rescan_finding_the_saved_mtime_loads_nothing() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.save("a.md", "saved");
    h.rescan();

    let _ = h.build();

    assert!(h.next(SLOTS).is_none());
    assert_eq!(h.loads("a.md"), 1);
}

#[test]
fn a_load_landing_after_a_newer_save_is_dropped() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let _ = h.build();
    let load = h.start();
    let read = h.read(&load);

    h.save("a.md", "saved");

    assert!(!h.live.landed(load.ticket, read, &h.vault));
    assert_eq!(h.text("a.md").as_deref(), Some("saved"));
}

#[test]
fn a_stale_failure_keeps_the_newer_mtime() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "outside");
    let _ = h.build();
    let load = h.start();
    h.save("a.md", "saved");

    assert!(!h.fail(load.ticket));

    assert_eq!(h.text("a.md").as_deref(), Some("saved"));
    h.rescan();
    let _ = h.build();
    assert!(
        h.next(SLOTS).is_none(),
        "the save's mtime is still the one indexed"
    );
}

#[test]
fn a_note_queued_twice_is_loaded_once() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    h.fs.insert_file(path("a.md"), "a");
    h.write("b.md", "[[a]]");
    let _ = h.build();
    let Some(a) = h.next(1) else {
        panic!("a is first in vault order");
    };
    h.write("b.md", "[[c]]");
    let _ = h.build();

    assert!(h.next(1).is_none(), "a holds the one slot");
    h.land(&a);
    h.load_all();

    assert_eq!(
        h.loads("b.md"),
        2,
        "the first queue entry for b was superseded"
    );
    assert_eq!(h.text("b.md").as_deref(), Some("[[c]]"));
}

#[test]
fn a_note_queued_while_it_loads_waits_for_its_load() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.write("a.md", "[[c]]");
    let _ = h.build();
    let first = h.start();
    h.write("a.md", "[[b]]");
    let _ = h.build();

    assert!(h.next(SLOTS).is_none(), "one read per note at a time");
    assert_eq!(
        h.live.progress(),
        (1, 2),
        "a is loading and queued, and counts once"
    );

    let stale = h.read(&first);
    h.live.landed(first.ticket, stale, &h.vault);
    let second = h.start();
    assert_eq!(second.path, path("a.md"));
    h.land(&second);
    assert_eq!(h.live.progress(), (2, 2));
    assert_eq!(h.text("a.md").as_deref(), Some("[[b]]"));
}

#[test]
fn next_load_fills_at_most_the_slots_in_vault_order() {
    let files: Vec<String> = (0..5).map(|n| format!("n{n}.md")).collect();
    let files: Vec<(&str, &str)> = files.iter().map(|file| (file.as_str(), "")).collect();
    let mut h = Harness::new(&files);
    let _ = h.build();

    let (Some(n0), Some(n1)) = (h.next(2), h.next(2)) else {
        panic!("two slots");
    };
    assert_eq!((n0.path.clone(), n1.path), (path("n0.md"), path("n1.md")));
    assert!(h.next(2).is_none(), "two slots, two loads");
    assert_eq!(h.live.progress(), (0, 5));

    h.land(&n0);
    let Some(n2) = h.next(2) else {
        panic!("n0's slot is free");
    };
    assert_eq!(n2.path, path("n2.md"));
    assert_eq!(h.live.progress(), (1, 5));
}

#[test]
fn a_save_landing_during_a_load_keeps_its_slot() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.fs.insert_file(path("a.md"), "outside");
    h.write("b.md", "outside");
    let _ = h.build();
    let Some(_a) = h.next(1) else {
        panic!("a is first in vault order");
    };

    h.save("a.md", "saved");

    assert!(h.next(1).is_none(), "a's load still holds the slot");
}

#[test]
fn a_load_in_flight_for_a_note_gone_is_not_pending() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    h.fs.insert_file(path("a.md"), "changed");
    h.write("b.md", "changed");
    let _ = h.build();
    let Some(load) = h.next(1) else {
        panic!("a is first in vault order");
    };
    assert_eq!(h.live.progress(), (1, 3), "a is loading, b is queued");

    h.fs.remove(&path("a.md"));
    h.remove("b.md");
    let _ = h.build();

    assert_eq!(h.live.progress(), (1, 1), "c, the only note, is loaded");
    assert!(!h.live.is_building());
    h.land(&load);
    assert_eq!(h.live.progress(), (1, 1));
}

#[test]
fn a_note_reloading_during_its_load_counts_once() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.fs.insert_file(path("a.md"), "[[c]]");
    h.write("c.md", "");
    let _ = h.build();
    let Some(first) = h.next(1) else {
        panic!("a is first in vault order");
    };
    assert_eq!(
        h.live.progress(),
        (1, 3),
        "a is loading, c is queued behind it"
    );

    h.fs.insert_file(path("a.md"), "[[b]]");
    h.write("d.md", "");
    let _ = h.build();
    assert_eq!(
        h.live.progress(),
        (1, 4),
        "a is loading and queued again, c and d are queued"
    );

    h.land(&first);
    h.load_all();
    assert_eq!(h.live.progress(), (4, 4));
    assert_eq!(h.text("a.md").as_deref(), Some("[[b]]"));
}

#[test]
fn a_load_failing_after_a_layout_is_retried() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.build();
    let load = h.start();
    h.write("c.md", "");
    assert!(h.build().layout);
    let c = h.start();
    h.land(&c);

    assert!(!h.fail(load.ticket), "a retry changes nothing yet");

    let retry = h.start();
    assert_eq!(retry.path, path("a.md"));
    h.land(&retry);
    assert_eq!(h.text("a.md").as_deref(), Some("new"));
}

#[test]
fn a_load_failing_after_only_mtimes_forgets_the_note() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.build();
    let load = h.start();
    h.write("b.md", "moved");
    assert!(!h.build().layout);
    h.load_all();

    assert!(h.fail(load.ticket));

    assert_eq!(h.text("a.md"), None, "its text is dropped");
    assert!(!h.live.index().is_indexed(h.id("a.md")));
    assert!(h.next(SLOTS).is_none(), "not retried before the next apply");
}

#[test]
fn a_forgotten_note_is_queued_at_the_next_apply_of_either_kind() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let _ = h.build();
    let load = h.start();
    h.write("b.md", "moved");
    let _ = h.build();
    h.load_all();
    assert!(h.fail(load.ticket));

    h.write("b.md", "again");
    assert!(!h.build().layout);
    h.load_all();
    assert_eq!(
        h.text("a.md").as_deref(),
        Some("new"),
        "an mtime apply retries it"
    );

    let load = {
        h.write("a.md", "newer");
        let _ = h.build();
        h.start()
    };
    h.write("b.md", "once more");
    let _ = h.build();
    h.load_all();
    assert!(h.fail(load.ticket));
    h.write("c.md", "");
    assert!(h.build().layout);
    h.load_all();
    assert_eq!(
        h.text("a.md").as_deref(),
        Some("newer"),
        "a layout apply retries it"
    );
}

#[test]
fn a_failed_load_behind_a_save_still_parsing_keeps_the_note() {
    let mut h = Harness::loaded(&[("a.md", "old words"), ("b.md", "")]);
    h.write("a.md", "changed");
    h.write("c.md", "");
    let _ = h.build();
    let load = h.start();
    let c = h.start();
    h.land(&c);
    let (_parsing, _) = h.saving("a.md", "saved");
    // The vault still lists the note: the batch that drops it has not landed.
    h.fs.remove(&path("a.md"));

    assert!(!h.fail(load.ticket));

    assert_eq!(h.text("a.md").as_deref(), Some("old words"));
    assert!(h.next(SLOTS).is_none(), "the save brings the text");
}

#[test]
fn a_failed_load_behind_a_queued_reload_keeps_the_note() {
    let mut h = Harness::loaded(&[("a.md", "[[b]] old"), ("b.md", "")]);
    h.write("a.md", "first");
    h.write("c.md", "");
    let _ = h.build();
    let first = h.start();
    h.write("a.md", "second");
    h.write("d.md", "");
    let _ = h.build();
    // The vault still lists the note: the batch that drops it has not landed.
    h.fs.remove(&path("a.md"));

    assert!(!h.fail(first.ticket));

    assert_eq!(h.text("a.md").as_deref(), Some("[[b]] old"));
    let started: Vec<PathBuf> = iter::from_fn(|| h.next(SLOTS))
        .map(|load| load.path)
        .collect();
    assert!(
        started.contains(&path("a.md")),
        "the queued reload still runs"
    );
}

#[test]
fn a_save_landing_before_its_note_joins_is_reloaded_by_the_next_build() {
    let mut h = Harness::new(&[("a.md", "[[b]]"), ("b.md", "")]);
    let a = h.id("a.md");
    let mtime = h.mtime_on_disk("a.md");
    let ticket = h.live.saved(a, mtime);

    h.live.landed(
        ticket,
        Ok(NoteContent::parse("[[b]]".into(), mtime)),
        &h.vault,
    );
    let _ = h.build();
    h.load_all();

    assert_eq!(h.text("a.md").as_deref(), Some("[[b]]"));
    assert_eq!(h.loads("a.md"), 1, "the build reloads it");
}
