//! `LiveIndex::turn`: the work a turn's end returns, and when it notifies.

use std::path::Path;

use inkra_vault::Vault;

use super::LiveIndex;
use super::tests::{Harness, path, paths};
use crate::test_support::{ROOT, stamp};

/// Built, every note loaded, and the settle applied, each in its own turn.
fn indexed(files: &[(&str, &str)]) -> Harness {
    let mut h = Harness::loaded(files);
    assert!(h.run_merge().indexed.is_some());
    h
}

#[test]
#[should_panic(expected = "a load slot")]
fn a_live_index_without_a_load_slot_is_refused() {
    let _ = LiveIndex::new(0);
}

#[test]
fn a_landed_load_refills_its_slot_in_the_same_turn() {
    let mut h = Harness::with_slots(&[("a.md", ""), ("b.md", "")], 1);
    assert_eq!(paths(&h.built().loads), [path("a.md")]);

    let landed = h.land_pending(0);

    assert_eq!(paths(&landed.loads), [path("b.md")]);
}

#[test]
fn a_turn_fills_at_most_the_slots_in_vault_order() {
    let files: Vec<String> = (0..5).map(|n| format!("n{n}.md")).collect();
    let files: Vec<(&str, &str)> = files.iter().map(|file| (file.as_str(), "")).collect();
    let mut h = Harness::with_slots(&files, 2);

    assert_eq!(paths(&h.built().loads), [path("n0.md"), path("n1.md")]);
    assert_eq!(h.live.progress(), (0, 5));

    assert_eq!(
        paths(&h.land_pending(0).loads),
        [path("n2.md")],
        "n0's slot"
    );
    assert_eq!(h.live.progress(), (1, 5));
}

#[test]
fn a_turn_that_changes_no_answer_does_not_notify() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    assert!(h.built().notify, "a layout");
    assert!(h.land_pending(0).notify, "a note's content");
    let _ = h.land_pending(0);

    let settled = h.run_merge();

    assert!(settled.indexed.is_some());
    assert!(!settled.notify, "a merge changes no answer");
    assert!(!h.idle_turn().notify, "an idle turn");
}

#[test]
fn an_mtimes_apply_notifies_through_progress_alone() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    let revision = h.live.revision();

    let applied = h.built();

    assert!(!applied.layout);
    assert_eq!(h.live.revision(), revision, "no answer moved yet");
    assert!(applied.notify, "a is loading again");
}

#[test]
fn an_idle_turn_retires_nothing() {
    let mut h = Harness::new(&[("a.md", "alpha")]);
    assert!(
        h.built().retired.is_some(),
        "the note set the build replaced"
    );
    assert!(
        h.land_pending(0).retired.is_none(),
        "a load replaces nothing"
    );
    assert!(
        h.run_merge().retired.is_some(),
        "the words the settle replaced"
    );

    assert!(h.idle_turn().retired.is_none());
}

#[test]
fn a_save_minted_in_its_own_turn_does_no_work() {
    let mut h = indexed(&[("a.md", "alpha"), ("b.md", "beta")]);
    let a = h.id("a.md");

    let (_, minted) = h.turn(|t| t.saved(a, stamp(9)));

    assert!(minted.loads.is_empty());
    assert!(h.pending.build.is_none(), "no build");
    assert!(h.pending.merge.is_none(), "no merge");
    assert!(minted.retired.is_none());
    assert!(!minted.layout && !minted.notify && minted.indexed.is_none());
}

#[test]
fn a_save_minted_with_a_free_slot_refills_it() {
    let mut h = Harness::loaded(&[("a.md", "old"), ("b.md", "")]);
    h.write("a.md", "new");
    // Steps, not a turn: a turn's end refills, so no turn leaves a note queued beside a free slot.
    let Some(job) = h.live.start_build(h.vault.shared_entries()) else {
        panic!("no build runs");
    };
    drop(h.live.apply_build(job.run(), &h.vault).retired);
    let b = h.id("b.md");

    let (_, minted) = h.turn(|t| t.saved(b, stamp(9)));

    assert_eq!(paths(&minted.loads), [path("a.md")]);
}

#[test]
fn a_dirty_build_returns_its_rerun_read_from_the_live_vault() {
    let mut h = Harness::loaded(&[("a.md", ""), ("b.md", "")]);
    h.write("c.md", "");
    let _ = h.vault_changed();
    h.write("d.md", "");
    let _ = h.vault_changed();

    let applied = h.run_build();

    assert!(applied.layout, "c joined before the read");
    assert_eq!(h.live.index().names().len(), 3, "read before d");
    assert!(h.run_build().layout, "the rerun reads d");
    assert_eq!(h.live.index().names().len(), 4);
    assert!(h.pending.build.is_none(), "the rerun was clean");
}

#[test]
fn a_vault_change_before_the_scan_starts_no_build() {
    let mut h = Harness::new(&[("a.md", "")]);
    h.vault = Vault::new(Path::new(ROOT));

    let _ = h.vault_changed();
    assert!(h.pending.build.is_none());

    h.rescan();
    let _ = h.vault_changed();
    assert!(h.pending.build.is_some(), "scanned");
}

#[test]
fn a_vault_change_before_the_scan_does_no_work() {
    let mut h = Harness::new(&[("a.md", "")]);
    h.vault = Vault::new(Path::new(ROOT));

    // The vault notifies once per scan chunk, so this turn runs once per chunk.
    let early = h.vault_changed();

    assert!(early.loads.is_empty());
    assert!(h.pending.merge.is_none(), "no merge");
    assert!(early.retired.is_none());
    assert!(!early.layout && !early.notify && early.indexed.is_none());
}

#[test]
#[should_panic(expected = "one build applies per turn")]
fn a_second_build_in_one_turn_is_refused() {
    let mut h = Harness::new(&[("a.md", "")]);
    let mut other = LiveIndex::new(1);
    let (Some(first), Some(second)) = (
        h.live.start_build(h.vault.shared_entries()),
        other.start_build(h.vault.shared_entries()),
    ) else {
        panic!("both idle");
    };
    let (first, second) = (first.run(), second.run());

    let _ = h.turn(|t| {
        t.built(first);
        t.built(second);
    });
}

#[test]
#[should_panic(expected = "one merge applies per turn")]
fn a_second_merge_in_one_turn_is_refused() {
    let files = [("a.md", "alpha")];
    let (mut h, mut other) = (Harness::loaded(&files), Harness::loaded(&files));
    let (Some(first), Some(second)) = (h.pending.merge.take(), other.pending.merge.take()) else {
        panic!("both settling");
    };
    let (first, second) = (first.run(), second.run());

    let _ = h.turn(|t| {
        t.merged(first);
        t.merged(second);
    });
}

#[test]
fn a_stale_note_loads_again_and_moves_no_revision() {
    let mut h = indexed(&[("a.md", "alpha"), ("b.md", "beta")]);
    let (a, revision) = (h.id("a.md"), h.live.revision());

    let ((), effects) = h.turn(|t| t.stale(&[a]));

    assert_eq!(paths(&effects.loads), [path("a.md")]);
    assert_eq!(h.live.revision(), revision);
}

#[test]
fn a_stale_note_the_index_does_not_hold_queues_nothing() {
    let mut h = indexed(&[("a.md", "alpha")]);
    h.write("c.md", "gamma");
    let c = h.id("c.md");

    let ((), effects) = h.turn(|t| t.stale(&[c]));

    assert!(effects.loads.is_empty());
}

#[test]
fn a_stale_note_already_queued_keeps_its_place() {
    let mut h = Harness::with_slots(&[("a.md", ""), ("b.md", ""), ("c.md", "")], 1);
    assert_eq!(paths(&h.built().loads), [path("a.md")]);
    let b = h.id("b.md");
    let _ = h.turn(|t| t.stale(&[b]));

    assert_eq!(
        paths(&h.land_pending(0).loads),
        [path("b.md")],
        "b before c"
    );
}
