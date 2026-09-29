//! Random interleavings of vault changes, builds held across them, saves and loads landing in
//! any order: the index ends as a fresh build of the final vault answers.

use std::collections::HashSet;

use futures::executor::block_on;
use inkra_vault::{EntryId, Fs, Kind};
use proptest::prelude::*;

use super::diff::notes;
use super::tests::{Harness, failure};
use super::{BuildJob, LiveIndex, Ticket};
use crate::NoteContent;
use crate::test_support::{ROOT, texts};

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
    StartBuild,
    ApplyBuild,
    StartLoad,
    Land(usize),
    Fail(usize),
    /// A save: the file is written, and its parse lands later.
    Save(usize, usize),
    LandSave(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let any = 0..8usize;
    prop_oneof![
        1 => (any.clone(), 0..TEXTS.len()).prop_map(|(at, text)| Op::Write(at, text)),
        1 => (0..NAMES.len()).prop_map(Op::Create),
        1 => (any.clone(), 0..NAMES.len()).prop_map(|(at, name)| Op::Rename(at, name)),
        1 => any.clone().prop_map(Op::Trash),
        2 => Just(Op::StartBuild),
        2 => Just(Op::ApplyBuild),
        3 => Just(Op::StartLoad),
        3 => any.clone().prop_map(Op::Land),
        1 => any.clone().prop_map(Op::Fail),
        1 => (any.clone(), 0..TEXTS.len()).prop_map(|(at, text)| Op::Save(at, text)),
        1 => any.prop_map(Op::LandSave),
    ]
}

/// The harness and the work in flight: at most one build, any loads and save parses.
struct Model {
    h: Harness,
    build: Option<BuildJob>,
    /// Each read when its load started: a read landing after a rename swapped two paths would
    /// hold the other note's text, a race the scheduler does not guard (spec, As built).
    loads: Vec<(Ticket, inkra_vault::Result<NoteContent>)>,
    saves: Vec<(Ticket, NoteContent)>,
}

impl Model {
    fn new() -> Self {
        Self {
            h: Harness::new(&[
                ("a.md", "[[b]]"),
                ("b.md", "[[c]] [[a]]"),
                ("d/e.md", "[[a]]"),
            ]),
            build: None,
            loads: Vec::new(),
            saves: Vec::new(),
        }
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

    fn start_build(&mut self) {
        let job = self.h.live.start_build(self.h.vault.shared_entries());
        if let Some(job) = job {
            assert!(self.build.is_none(), "one build at a time");
            self.build = Some(job);
        }
    }

    fn apply_build(&mut self) -> bool {
        let Some(job) = self.build.take() else {
            return false;
        };
        let applied = self.h.live.apply_build(job.run(), &self.h.vault);
        drop(applied.retired);
        // As `IndexSync` does: a change during the build asks for another.
        if applied.dirty {
            self.start_build();
        }
        true
    }

    fn start_load(&mut self) -> bool {
        let Some(load) = self.h.next(SLOTS) else {
            return false;
        };
        let read = self.h.read(&load);
        self.loads.push((load.ticket, read));
        true
    }

    fn land(&mut self, at: usize, fail: bool) -> bool {
        if self.loads.is_empty() {
            return false;
        }
        let (ticket, read) = self.loads.remove(at % self.loads.len());
        let loaded = if fail { Err(failure()) } else { read };
        self.h.live.landed(ticket, loaded, &self.h.vault);
        true
    }

    fn land_save(&mut self, at: usize) -> bool {
        if self.saves.is_empty() {
            return false;
        }
        let (ticket, content) = self.saves.remove(at % self.saves.len());
        self.h.live.landed(ticket, Ok(content), &self.h.vault);
        true
    }

    fn run(&mut self, op: &Op) {
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
            Op::StartBuild => self.start_build(),
            Op::ApplyBuild => {
                self.apply_build();
            }
            Op::StartLoad => {
                self.start_load();
            }
            Op::Land(at) => {
                self.land(at, false);
            }
            Op::Fail(at) => {
                self.land(at, true);
            }
            Op::Save(at, text) => {
                if let Some((id, path)) = self.note(at) {
                    self.h.fs.insert_file(&path, TEXTS[text]);
                    // As `NoteStore` re-keys the entry after a save, the vault holds the saved mtime.
                    self.h.rescan();
                    let Some(mtime) = self.h.vault.entry(id).map(|entry| entry.mtime) else {
                        panic!("a saved note stays listed");
                    };
                    let ticket = self.h.live.saved(id, mtime);
                    self.saves
                        .push((ticket, NoteContent::parse(TEXTS[text].into(), mtime)));
                }
            }
            Op::LandSave(at) => {
                self.land_save(at);
            }
        }
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
        let stray: Vec<&EntryId> = pending.iter().filter(|id| !applied.contains(id)).collect();
        prop_assert!(
            stray.is_empty(),
            "pending, not in the applied set: {stray:?}"
        );
        Ok(())
    }

    /// Runs every build, parse and load in flight, then a last build, until nothing is left;
    /// false when that takes more steps than the vault can need, as a build that stays dirty would.
    fn drain(&mut self) -> bool {
        self.start_build();
        for _ in 0..1_000 {
            let busy = self.apply_build() | self.land(0, false) | self.land_save(0);
            if !busy && !self.start_load() {
                return true;
            }
        }
        false
    }
}

/// What a reader sees of a vault note: whether it is indexed, its text, its backlinks.
type Answer = (EntryId, bool, Option<String>, Vec<(EntryId, usize)>);

fn answers(h: &Harness) -> Vec<Answer> {
    let index = h.live.index();
    let held = texts(&index.corpus());
    h.vault
        .entries()
        .filter(|entry| entry.kind == Kind::Note)
        .map(|entry| {
            let id = entry.id;
            let text = held
                .iter()
                .find(|(held, _)| *held == id)
                .map(|(_, text)| text.clone());
            let backlinks = index
                .backlinks(id)
                .into_iter()
                .map(|backlink| (backlink.source, backlink.links.len()))
                .collect();
            (id, index.is_indexed(id), text, backlinks)
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
            model.run(op);
            model.check()?;
        }
        prop_assert!(model.drain(), "the index settles");
        model.check()?;

        let mut fresh = Harness {
            fs: std::mem::take(&mut model.h.fs),
            vault: model.h.vault.clone(),
            live: LiveIndex::default(),
        };
        fresh.build();
        fresh.load_all();

        prop_assert!(!model.h.live.is_building());
        prop_assert_eq!(model.h.live.progress(), fresh.live.progress());
        prop_assert_eq!(answers(&model.h), answers(&fresh));
    }
}
