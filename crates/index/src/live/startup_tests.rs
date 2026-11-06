//! L7: the startup settle and the one `indexed` report, stepped by each turn's end.

use super::Merge;
use super::tests::Harness;
use crate::test_support::content;

/// Lands every pending load, one turn each; the merges the turns started, not run, and whether one reported `indexed`.
fn load_all_in_turns(h: &mut Harness) -> (Vec<Merge>, bool) {
    let (mut merges, mut indexed) = (Vec::new(), false);
    while !h.pending.loads.is_empty() {
        indexed |= h.land_pending(0).indexed;
        merges.extend(h.pending.merge.take());
    }
    (merges, indexed)
}

fn settle(h: &mut Harness) -> Merge {
    let (mut merges, indexed) = load_all_in_turns(h);
    assert!(!indexed, "not before the settle");
    let Some(merge) = merges.pop() else {
        panic!("the last load's turn starts the settle");
    };
    assert!(merges.is_empty(), "below the policy, nothing merged before");
    merge
}

#[test]
fn the_load_ending_settles_then_reports_indexed_once() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    let first = h.built();
    assert!(
        h.pending.merge.is_none() && !first.indexed,
        "the loads are queued"
    );
    let merge = settle(&mut h);

    let end = h.merged(merge);

    assert!(end.indexed);
    assert!(h.live.index().words_settled());
    let later = h.idle_turn();
    assert!(h.pending.merge.is_none() && !later.indexed, "reported once");
}

#[test]
fn a_settle_with_nothing_to_merge_reports_indexed_in_that_turn() {
    let mut h = Harness::new(&[("image.png", "")]);

    let end = h.built();

    assert!(h.pending.merge.is_none());
    assert!(end.indexed);
}

#[test]
fn a_save_during_the_settle_stays_fresh_and_indexed_is_reported() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "")]);
    let _ = h.built();
    let merge = settle(&mut h);
    let a = h.id("a.md");
    let during = content("during", crate::test_support::mtime(9));
    let ((), end) = h.turn(|t| {
        let ticket = t.saved(a, crate::test_support::mtime(9));
        t.landed(ticket, Ok(during));
    });
    assert!(
        h.pending.merge.is_none() && !end.indexed,
        "the settle still runs"
    );

    let end = h.merged(merge);

    assert!(end.indexed, "the settle's apply ends the settle");
    assert!(!h.live.index().words_settled(), "the save waits in fresh");
}

#[test]
fn building_again_before_indexed_settles_again() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    let _ = h.built();
    let merge = settle(&mut h);
    h.fs.insert_file(super::tests::path("c.md"), "gamma");
    h.rescan();
    let _ = h.built();
    assert!(h.live.is_building());

    let end = h.merged(merge);
    assert!(!end.indexed, "c is loading");
    assert!(
        h.pending.merge.is_none(),
        "one fresh note is below the policy"
    );

    let merge = settle(&mut h);
    assert!(h.merged(merge).indexed);
}

#[test]
fn a_reload_after_indexed_starts_no_settle() {
    // Enough merged notes that rewriting `a` leaves the dead share below the policy's.
    let fillers: Vec<String> = (0..10).map(|n| format!("n{n}.md")).collect();
    let mut files = vec![("a.md", "alpha"), ("b.md", "")];
    files.extend(fillers.iter().map(|file| (file.as_str(), "filler")));
    let mut h = Harness::new(&files);
    let _ = h.built();
    let merge = settle(&mut h);
    assert!(h.merged(merge).indexed);

    h.fs.insert_file(super::tests::path("a.md"), "changed");
    h.rescan();
    let _ = h.built();
    let (merges, indexed) = load_all_in_turns(&mut h);

    assert!(merges.is_empty(), "a reload waits for the policy");
    assert!(!indexed);
    assert!(
        !h.live.index().words_settled(),
        "the reloaded note is fresh"
    );
}

#[test]
fn a_load_past_the_policy_starts_a_due_merge() {
    let files: Vec<(String, String)> = (0..300)
        .map(|n| (format!("n{n:03}.md"), format!("w{n}")))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    let mut h = Harness::new(&files);
    let _ = h.built();

    let mut due = None;
    while !h.pending.loads.is_empty() {
        let _ = h.land_pending(0);
        if let Some(merge) = h.pending.merge.take() {
            due = Some(merge);
            break;
        }
    }

    let Some(merge) = due else {
        panic!("a merge came due");
    };
    assert!(h.live.is_building(), "due before the loads end");
    let end = h.merged(merge);
    assert!(!end.indexed, "a due merge is no settle");
}

#[test]
fn a_dirty_rebuild_ending_the_load_settles_before_indexed() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    let _ = h.built();
    let _ = h.land_pending(0);
    assert!(h.live.is_building(), "b is loading");
    // The build drops b, the last note loading; a change while it runs makes it dirty.
    h.remove("b.md");
    let _ = h.vault_changed();
    let _ = h.vault_changed();

    let applied = h.run_build();

    assert!(!applied.indexed, "not before the settle");
    assert!(
        h.pending.merge.is_some(),
        "the settle starts beside the rerun"
    );
    assert!(h.pending.build.is_some(), "the rerun");
    assert!(h.run_merge().indexed);
    assert!(h.live.index().words_settled());
    assert!(!h.run_build().indexed, "reported once");
}
