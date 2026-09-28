use futures::FutureExt as _;
use futures::channel::oneshot;
use gpui::TestAppContext;

use super::{advance, listed, open, scans, search, selected, with_texts};
use crate::index::{BUILDING_REFRESH, IndexSync};
use crate::test_support::entry_id;

#[gpui::test]
fn trashing_a_listed_note_removes_its_header_and_hits(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "plan")]);
    let mut opened = open(cx, &fs);
    search(&mut opened, "p l a n");
    let a = entry_id(&opened.notes, &mut opened.visual, "a");

    let trashed = opened
        .notes
        .update(&mut opened.visual, |notes, cx| notes.trash(a, cx));
    opened.visual.run_until_parked();
    drop(trashed);

    assert_eq!(listed(&mut opened), ["# b (1)", "1: plan"]);
}

#[gpui::test]
fn a_refresh_during_loads_keeps_the_selected_hit(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "plan\nplan"), ("c.md", "")]);
    let a = fs.block_load("/vault/a.md");
    // Keeps the index building, so the refresh is throttled.
    let _c = fs.block_load("/vault/c.md");
    let mut opened = open(cx, &fs);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened), ["# b (2)", "1: plan", "2: plan"]);
    opened.visual.simulate_keystrokes("down");

    a.release();
    advance(&mut opened, BUILDING_REFRESH);

    assert_eq!(
        listed(&mut opened),
        ["# a (1)", "1: plan", "# b (2)", "1: plan", "2: plan"]
    );
    assert_eq!(selected(&mut opened), Some(4), "still b's second hit");
}

#[gpui::test]
fn a_refresh_during_a_scan_waits_for_it(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan")]);
    let mut opened = open(cx, &fs);
    let (release, hold) = oneshot::channel::<()>();
    opened.picker.update(&mut opened.visual, |picker, _| {
        picker.delegate.runs.hold(Some(hold.shared()));
    });
    opened.visual.simulate_keystrokes("p");
    let cancel = opened.picker.read_with(&opened.visual, |picker, _| {
        picker.delegate.runs.cancel_flag()
    });
    let before = scans(&mut opened);

    let links = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.links.clone());
    links.update(&mut opened.visual, IndexSync::simulate_load);
    opened.visual.run_until_parked();
    assert!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "the scan runs on"
    );

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(scans(&mut opened), before + 1, "one re-run after it");
    assert_eq!(listed(&mut opened), ["# a (1)", "1: plan"]);
}
