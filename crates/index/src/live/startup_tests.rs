//! L7: the startup settle and the one `indexed` report, stepped by `end_turn`.

use super::tests::Harness;
use super::{Merge, TurnEnd};

/// Lands every queued load, one turn each; the merges the turns started, and whether one reported `indexed`.
fn load_all_in_turns(h: &mut Harness) -> (Vec<Merge>, bool) {
    let (mut merges, mut indexed) = (Vec::new(), false);
    while let Some(load) = h.next(usize::MAX) {
        h.land(&load);
        let TurnEnd {
            merge,
            indexed: now,
        } = h.turn();
        merges.extend(merge);
        indexed |= now;
    }
    (merges, indexed)
}

/// Applies `merge` in its own turn.
fn merged(h: &mut Harness, merge: Merge) -> TurnEnd {
    drop(h.live.merged(merge.run()));
    h.turn()
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
    let _ = h.build();
    let first = h.turn();
    assert!(
        first.merge.is_none() && !first.indexed,
        "the loads are queued"
    );
    let merge = settle(&mut h);

    let end = merged(&mut h, merge);

    assert!(end.indexed);
    assert!(h.live.index().words_settled());
    let later = h.turn();
    assert!(later.merge.is_none() && !later.indexed, "reported once");
}

#[test]
fn a_settle_with_nothing_to_merge_reports_indexed_in_that_turn() {
    let mut h = Harness::new(&[("image.png", "")]);
    let _ = h.build();

    let end = h.turn();

    assert!(end.merge.is_none());
    assert!(end.indexed);
}

#[test]
fn a_save_during_the_settle_stays_fresh_and_indexed_is_reported() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "")]);
    let _ = h.build();
    let merge = settle(&mut h);
    let a = h.id("a.md");
    let ticket = h.live.saved(a, crate::test_support::mtime(9));
    let during = crate::NoteContent::parse("during".into(), crate::test_support::mtime(9));
    h.live.landed(ticket, Ok(during), &h.vault);
    let end = h.turn();
    assert!(end.merge.is_none() && !end.indexed, "the settle still runs");

    let end = merged(&mut h, merge);

    assert!(end.indexed, "the settle's apply ends the settle");
    assert!(!h.live.index().words_settled(), "the save waits in fresh");
}

#[test]
fn building_again_before_indexed_settles_again() {
    let mut h = Harness::new(&[("a.md", "alpha"), ("b.md", "beta")]);
    let _ = h.build();
    let merge = settle(&mut h);
    h.fs.insert_file(super::tests::path("c.md"), "gamma");
    h.rescan();
    let _ = h.build();
    assert!(h.live.is_building());

    let end = merged(&mut h, merge);
    assert!(!end.indexed, "c is loading");
    assert!(end.merge.is_none(), "one fresh note is below the policy");

    let merge = settle(&mut h);
    assert!(merged(&mut h, merge).indexed);
}

#[test]
fn a_reload_after_indexed_starts_no_settle() {
    // Enough merged notes that rewriting `a` leaves the dead share below the policy's.
    let fillers: Vec<String> = (0..10).map(|n| format!("n{n}.md")).collect();
    let mut files = vec![("a.md", "alpha"), ("b.md", "")];
    files.extend(fillers.iter().map(|file| (file.as_str(), "filler")));
    let mut h = Harness::new(&files);
    let _ = h.build();
    let merge = settle(&mut h);
    assert!(merged(&mut h, merge).indexed);

    h.fs.insert_file(super::tests::path("a.md"), "changed");
    h.rescan();
    let _ = h.build();
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
    let _ = h.build();

    let mut due = None;
    while let Some(load) = h.next(usize::MAX) {
        h.land(&load);
        if let Some(merge) = h.turn().merge {
            due = Some(merge);
            break;
        }
    }

    let Some(merge) = due else {
        panic!("a merge came due");
    };
    assert!(h.live.is_building(), "due before the loads end");
    let end = merged(&mut h, merge);
    assert!(!end.indexed, "a due merge is no settle");
}
