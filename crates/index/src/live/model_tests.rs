//! Random interleavings of vault changes, builds held across them, saves, loads and merges
//! landing in any order: the index ends as a fresh build of the final vault answers.

use std::collections::HashSet;

use futures::executor::block_on;
use inkra_vault::{EntryId, Fs, Kind, Stamp};
use proptest::prelude::*;

use super::diff::notes;
use super::tests::{Harness, Pending, failure, words_of};
use super::{LiveIndex, Status, Ticket, Turn};
use crate::NoteContent;
use crate::test_support::{ROOT, content, loaded};

const SLOTS: usize = 2;
const NAMES: &[&str] = &["a", "b", "c", "n1", "n2", "r1", "zz"];
const TEXTS: &[&str] = &["", "[[a]]", "[[b]] [[c]]", "[[n1]] words", "zz [[a]]"];

#[derive(Debug, Clone)]
enum Op {
    /// An edit outside the app.
    Write(usize, usize),
    Create(usize),
    Rename(usize, usize),
    Trash(usize),
    /// The vault's notify, in its own turn.
    VaultChanged,
    RunBuild,
    Land(usize),
    Fail(usize),
    /// A save: the file is written, and its parse lands later.
    Save(usize, usize),
    LandSave(usize),
    RunMerge,
}

fn op() -> impl Strategy<Value = Op> {
    let any = 0..8usize;
    prop_oneof![
        1 => (any.clone(), 0..TEXTS.len()).prop_map(|(at, text)| Op::Write(at, text)),
        1 => (0..NAMES.len()).prop_map(Op::Create),
        1 => (any.clone(), 0..NAMES.len()).prop_map(|(at, name)| Op::Rename(at, name)),
        1 => any.clone().prop_map(Op::Trash),
        2 => Just(Op::VaultChanged),
        2 => Just(Op::RunBuild),
        3 => any.clone().prop_map(Op::Land),
        1 => any.clone().prop_map(Op::Fail),
        1 => (any.clone(), 0..TEXTS.len()).prop_map(|(at, text)| Op::Save(at, text)),
        1 => any.prop_map(Op::LandSave),
        1 => Just(Op::RunMerge),
    ]
}

/// The harness, the jobs its turns returned, and the reads and parses waiting to land.
struct Model {
    h: Harness,
    /// Each read in the turn its load was returned: a read landing after a rename swapped two
    /// paths would hold the other note's text, a race the scheduler does not guard (spec, As built).
    loads: Vec<(Ticket, inkra_vault::Result<NoteContent>)>,
    saves: Vec<(Ticket, NoteContent)>,
    /// Turns that reported `indexed`.
    indexed: usize,
}

/// Every getter of `LiveIndex`: a turn that does not notify leaves it as it was (D5).
#[derive(Debug, PartialEq)]
struct Seen {
    revision: u64,
    progress: (usize, usize),
    status: Status,
    answers: Vec<Answer>,
}

impl Seen {
    fn of(h: &Harness) -> Self {
        Self {
            revision: h.live.revision(),
            progress: h.live.progress(),
            status: h.live.status(),
            answers: answers(h),
        }
    }
}

impl Model {
    fn new() -> Self {
        Self {
            h: Harness::with_slots(
                &[
                    ("a.md", "[[b]]"),
                    ("b.md", "[[c]] [[a]]"),
                    ("d/e.md", "[[a]]"),
                ],
                SLOTS,
            ),
            loads: Vec::new(),
            saves: Vec::new(),
            indexed: 0,
        }
    }

    /// Runs one turn, reading its loads and keeping its other jobs pending; a turn that does not
    /// notify changes no getter.
    fn turn<R>(&mut self, inputs: impl FnOnce(&mut Turn<'_>) -> R) -> Result<R, TestCaseError> {
        let before = Seen::of(&self.h);
        let (result, effects) = self.h.turn(inputs);
        drop(effects.retired);
        for load in std::mem::take(&mut self.h.pending.loads) {
            let read = self.h.read(&load);
            self.loads.push((load.ticket, read));
        }
        self.indexed += usize::from(effects.indexed);
        if !effects.notify {
            prop_assert_eq!(before, Seen::of(&self.h), "a turn that did not notify");
        }
        Ok(result)
    }

    /// The vault's notes, in vault order, with their absolute paths.
    fn notes(&self) -> Vec<(EntryId, std::path::PathBuf)> {
        let vault = &self.h.vault;
        vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .filter_map(|entry| Some((entry.id, vault.absolute(entry.id)?)))
            .collect()
    }

    fn note(&self, at: usize) -> Option<(EntryId, std::path::PathBuf)> {
        let notes = self.notes();
        (!notes.is_empty()).then(|| notes[at % notes.len()].clone())
    }

    fn vault_changed(&mut self) -> Result<(), TestCaseError> {
        // Typed, or clippy asks for the path `Turn::vault_changed`, which binds one lifetime and fails the bound.
        self.turn(|t: &mut Turn<'_>| t.vault_changed())
    }

    fn run_build(&mut self) -> Result<bool, TestCaseError> {
        let Some(job) = self.h.pending.build.take() else {
            return Ok(false);
        };
        let built = job.run();
        self.turn(|t| t.built(built))?;
        Ok(true)
    }

    fn land(&mut self, at: usize, fail: bool) -> Result<bool, TestCaseError> {
        if self.loads.is_empty() {
            return Ok(false);
        }
        let (ticket, read) = self.loads.remove(at % self.loads.len());
        let loaded = if fail { Err(failure()) } else { read };
        self.turn(|t| t.landed(ticket, loaded))?;
        Ok(true)
    }

    fn land_save(&mut self, at: usize) -> Result<bool, TestCaseError> {
        if self.saves.is_empty() {
            return Ok(false);
        }
        let (ticket, content) = self.saves.remove(at % self.saves.len());
        self.turn(|t| t.landed(ticket, Ok(content)))?;
        Ok(true)
    }

    fn run_merge(&mut self) -> Result<bool, TestCaseError> {
        let Some(merge) = self.h.pending.merge.take() else {
            return Ok(false);
        };
        let merged = merge.run();
        self.turn(|t| t.merged(merged))?;
        Ok(true)
    }

    fn run(&mut self, op: &Op) -> Result<(), TestCaseError> {
        match *op {
            Op::Write(at, text) => {
                if let Some((_, path)) = self.note(at) {
                    self.h.fs.insert_file(path, TEXTS[text]);
                    self.h.rescan();
                }
            }
            Op::Create(name) => {
                self.h
                    .fs
                    .insert_file(format!("{ROOT}/{}.md", NAMES[name]), "");
                self.h.rescan();
            }
            Op::Rename(at, name) => {
                if let Some((_, path)) = self.note(at) {
                    let to = format!("{ROOT}/{}.md", NAMES[name]);
                    let _ = block_on(self.h.fs.rename(&path, std::path::Path::new(&to)));
                    self.h.rescan();
                }
            }
            Op::Trash(at) => {
                if let Some((_, path)) = self.note(at) {
                    self.h.fs.remove(&path);
                    self.h.rescan();
                }
            }
            Op::VaultChanged => self.vault_changed()?,
            Op::RunBuild => {
                self.run_build()?;
            }
            Op::Land(at) => {
                self.land(at, false)?;
            }
            Op::Fail(at) => {
                self.land(at, true)?;
            }
            Op::Save(at, text) => {
                if let Some((id, path)) = self.note(at) {
                    self.h.fs.insert_file(&path, TEXTS[text]);
                    // As `NoteStore` re-keys the entry after a save, the vault holds the saved mtime.
                    self.h.rescan();
                    let Some(mtime) = self.h.vault.entry(id).map(|entry| entry.mtime) else {
                        panic!("a saved note stays listed");
                    };
                    let ticket = self.turn(|t| t.saved(id, mtime))?;
                    self.saves.push((ticket, content(TEXTS[text], mtime)));
                }
            }
            Op::LandSave(at) => {
                self.land_save(at)?;
            }
            Op::RunMerge => {
                self.run_merge()?;
            }
        }
        Ok(())
    }

    /// No more loads than slots, and every pending note a note of the applied build.
    fn check(&self) -> Result<(), TestCaseError> {
        let live = &self.h.live;
        prop_assert!(live.loading.len() <= SLOTS, "one load per slot");
        let applied: HashSet<EntryId> = live
            .last
            .as_deref()
            .map(|last| notes(last).map(|entry| entry.id).collect())
            .unwrap_or_default();
        let pending: HashSet<EntryId> = live
            .queued
            .keys()
            .copied()
            .chain(
                live.loading
                    .iter()
                    .filter(|load| load.live)
                    .map(|load| load.id),
            )
            .collect();
        let (loaded, total) = live.progress();
        prop_assert!(
            pending.len() <= total,
            "{} pending of {total}",
            pending.len()
        );
        prop_assert_eq!(loaded, total - pending.len());
        match live.status() {
            Status::Scanning => prop_assert!(live.last.is_none(), "scanning after a build"),
            Status::Indexing {
                loaded: shown,
                total: of,
            } => {
                prop_assert!(live.last.is_some(), "indexing before a build");
                prop_assert_eq!((shown, of), (loaded, total));
                prop_assert!(shown < of, "indexing {shown} of {of}");
            }
            Status::Ready => {
                prop_assert!(live.last.is_some(), "ready before a build");
                prop_assert_eq!(loaded, total);
            }
        }
        let stray: Vec<&EntryId> = pending.iter().filter(|id| !applied.contains(id)).collect();
        prop_assert!(
            stray.is_empty(),
            "pending, not in the applied set: {stray:?}"
        );
        Ok(())
    }

    /// A last vault change, then every job its turns return, until none is left; false when that
    /// takes more steps than the vault can need, as a build that stays dirty would.
    fn drain(&mut self) -> Result<bool, TestCaseError> {
        self.vault_changed()?;
        for _ in 0..1_000 {
            let busy =
                self.run_build()? | self.land(0, false)? | self.land_save(0)? | self.run_merge()?;
            if !busy {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// What a reader sees of a vault note: its path, whether it is indexed, its stamp, its
/// `(word, count)`s, its backlinks.
type Answer = (
    String,
    bool,
    Option<Stamp>,
    Vec<(String, u32)>,
    Vec<(EntryId, usize)>,
);

fn answers(h: &Harness) -> Vec<Answer> {
    let index = h.live.index();
    let stamps = loaded(&index.snapshot());
    let words = index.words().entries();
    h.vault
        .entries()
        .filter(|entry| entry.kind == Kind::Note)
        .map(|entry| {
            let id = entry.id;
            let stamp = stamps
                .iter()
                .find(|(held, _)| *held == id)
                .map(|(_, stamp)| *stamp);
            let backlinks = index
                .backlinks(id)
                .into_iter()
                .map(|backlink| (backlink.source, backlink.links.len()))
                .collect();
            let path = entry.path.as_path().to_string_lossy().into_owned();
            (
                path,
                index.is_indexed(id),
                stamp,
                words_of(&words, id),
                backlinks,
            )
        })
        .collect()
}

proptest! {
    #[test]
    fn random_interleavings_leave_the_index_as_a_fresh_build(
        ops in prop::collection::vec(op(), 0..40),
    ) {
        let mut model = Model::new();
        for op in &ops {
            model.run(op)?;
            model.check()?;
        }
        prop_assert!(model.drain()?, "the index settles");
        model.check()?;
        prop_assert_eq!(model.indexed, 1, "indexed is reported once");

        let mut fresh = Harness {
            fs: std::mem::take(&mut model.h.fs),
            vault: model.h.vault.clone(),
            live: LiveIndex::new(SLOTS),
            pending: Pending::default(),
        };
        let _ = fresh.built();
        fresh.load_all();

        prop_assert!(!model.h.live.is_building());
        prop_assert_eq!(model.h.live.progress(), fresh.live.progress());
        prop_assert_eq!(answers(&model.h), answers(&fresh));
    }
}
