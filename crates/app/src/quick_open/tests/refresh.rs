use std::sync::atomic::Ordering;

use futures::FutureExt as _;
use futures::channel::oneshot;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext as _, Focusable, TestAppContext};
use inkra_vault::{Fs, Kind};

use super::{Opened, cancel_flag, footer, listed, open, search, with_texts};
use crate::index::{BUILDING_REFRESH, IndexSync};
use crate::keymap::key_bindings;
use crate::open::NoteActivated;
use crate::picker::Picker;
use crate::picker::runs::NOTES_CHANGED_REFRESH;
use crate::quick_open::QuickOpenDelegate;
use crate::test_support::{entry_id, unscanned_notes, vault};

/// Holds every scan that starts from now on until the sender is dropped or sent.
pub(super) fn hold_scans(opened: &mut Opened) -> oneshot::Sender<()> {
    let (release, hold) = oneshot::channel();
    opened.picker.update(&mut opened.visual, |picker, _| {
        picker.delegate.runs.hold(Some(hold.shared()));
    });
    release
}

pub(super) fn runs(opened: &mut Opened) -> usize {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.runs.started())
}

pub(super) fn selected(opened: &mut Opened) -> Option<usize> {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected())
}

pub(super) fn notify(opened: &mut Opened) {
    let links = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.links.clone());
    links.update(&mut opened.visual, IndexSync::simulate_load);
    opened.visual.run_until_parked();
}

pub(super) fn advance(opened: &mut Opened, by: std::time::Duration) {
    opened.visual.executor().advance_clock(by);
    opened.visual.run_until_parked();
}

#[gpui::test]
fn ten_notifies_during_a_scan_cause_no_cancel_and_one_rerun(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let release = hold_scans(&mut opened);
    opened.visual.simulate_keystrokes("p");
    let cancel = cancel_flag(&mut opened);
    let before = runs(&mut opened);

    for _ in 0..10 {
        notify(&mut opened);
        assert!(!cancel.load(Ordering::Relaxed), "the scan runs on");
    }
    assert_eq!(runs(&mut opened), before, "no scan starts beside it");

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(runs(&mut opened), before + 1, "one re-run for the ten");
    assert_eq!(listed(&mut opened), ["plan"]);
}

#[gpui::test]
fn a_keystroke_during_the_scan_cancels_it(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "pluto.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let release = hold_scans(&mut opened);
    opened.visual.simulate_keystrokes("p");
    let first = cancel_flag(&mut opened);
    notify(&mut opened);

    opened.visual.simulate_keystrokes("l a");
    let typed = runs(&mut opened);

    assert!(first.load(Ordering::Relaxed));
    let _ = release.send(());
    advance(&mut opened, super::SEARCH_DEBOUNCE);
    assert_eq!(listed(&mut opened), ["plan"]);
    assert_eq!(
        runs(&mut opened),
        typed,
        "the keystroke's scan read the newest index: no re-run for the notify"
    );
}

#[gpui::test]
fn the_selected_note_is_kept_across_a_load_refresh(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("travel.md", "We plan the trip."),
        ("plan.md", "A trip to Rome."),
        ("plan trip.md", ""),
    ]);
    let gate = fs.block_load("/vault/plan.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n space t r i p");
    assert_eq!(listed(&mut opened), ["plan trip", "travel"]);
    opened.visual.simulate_keystrokes("down");

    gate.release();
    opened.visual.run_until_parked();

    assert_eq!(listed(&mut opened), ["plan trip", "plan", "travel"]);
    assert_eq!(selected(&mut opened), Some(2), "still travel");
}

#[gpui::test]
fn a_note_loading_under_the_picker_joins_its_rows(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("travel.md", "We plan the trip."),
        ("other.md", ""),
        ("trip.md", ""),
    ]);
    let travel = fs.block_load("/vault/travel.md");
    let _other = fs.block_load("/vault/other.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "t r i p");
    assert_eq!(listed(&mut opened), ["trip"]);

    travel.release();
    opened.visual.run_until_parked();
    assert_eq!(
        listed(&mut opened),
        ["trip"],
        "still building: the refresh waits"
    );

    advance(&mut opened, BUILDING_REFRESH);
    assert_eq!(listed(&mut opened), ["trip", "travel"]);
}

#[gpui::test]
fn one_build_runs_one_scan(cx: &mut TestAppContext) {
    let fs = vault(&["plan a.md", "plan b.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    let before = runs(&mut opened);
    let a = entry_id(&opened.notes, &mut opened.visual, "plan a");

    let trashed = opened
        .notes
        .update(&mut opened.visual, |notes, cx| notes.trash(a, cx));
    opened.visual.run_until_parked();
    drop(trashed);
    advance(&mut opened, NOTES_CHANGED_REFRESH);

    assert_eq!(listed(&mut opened), ["plan b"]);
    assert_eq!(
        runs(&mut opened),
        before + 1,
        "the notify of the build's turn is skipped"
    );
}

#[gpui::test]
fn notes_changed_during_a_scan_reruns_once(cx: &mut TestAppContext) {
    let fs = vault(&["plan b.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let release = hold_scans(&mut opened);
    opened.visual.simulate_keystrokes("p");
    let cancel = cancel_flag(&mut opened);
    let before = runs(&mut opened);

    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, Kind::Note, "plan a", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    assert!(!cancel.load(Ordering::Relaxed), "the scan runs on");
    assert_eq!(runs(&mut opened), before);

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(runs(&mut opened), before + 1);
    assert_eq!(listed(&mut opened), ["plan a", "plan b"], "no second wait");
}

#[gpui::test]
fn the_footer_counts_the_notes_indexed_then_leaves(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md", "c.md"]);
    let b = fs.block_load("/vault/b.md");
    let c = fs.block_load("/vault/c.md");
    let mut opened = open(cx, &fs, None, &[]);
    assert_eq!(
        footer(&mut opened).as_deref(),
        Some("Indexing 1 of 3 notes")
    );

    let before = runs(&mut opened);

    b.release();
    opened.visual.run_until_parked();
    advance(&mut opened, BUILDING_REFRESH);
    assert_eq!(
        footer(&mut opened).as_deref(),
        Some("Indexing 2 of 3 notes")
    );
    assert_eq!(runs(&mut opened), before + 1, "the load's refresh redraws");
    assert!(opened.visual.debug_bounds("picker-footer").is_some());

    c.release();
    opened.visual.run_until_parked();
    assert_eq!(footer(&mut opened), None);
    assert!(
        opened.visual.debug_bounds("picker-footer").is_none(),
        "the last load redraws the picker"
    );
}

#[gpui::test]
fn the_selection_survives_the_footer_leaving(cx: &mut TestAppContext) {
    let fs = vault(&["plan a.md", "plan b.md", "other.md"]);
    let gate = fs.block_load("/vault/other.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    opened.visual.simulate_keystrokes("down");
    assert_eq!(listed(&mut opened), ["plan b", "plan a"]);
    assert!(opened.visual.debug_bounds("picker-footer").is_some());

    gate.release();
    opened.visual.run_until_parked();

    assert!(opened.visual.debug_bounds("picker-footer").is_none());
    assert_eq!(selected(&mut opened), Some(1), "still plan a");
}

#[gpui::test]
fn a_build_during_the_first_scan_does_not_cancel_it(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = vault(&["plan a.md", "f/plan b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    let (picker, visual) = cx.add_window_view(move |window, cx| {
        let delegate = QuickOpenDelegate::new(&links, None, Vec::new(), cx);
        Picker::new(delegate, window, cx)
    });
    let mut opened = Opened {
        notes,
        picker,
        visual: visual.clone(),
    };
    opened
        .visual
        .update(|window, cx| window.focus(&opened.picker.focus_handle(cx), cx));
    opened.visual.simulate_keystrokes("p");
    let release = hold_scans(&mut opened);
    let before = runs(&mut opened);

    gate.release();
    opened.visual.run_until_parked();
    assert_eq!(
        runs(&mut opened),
        before + 1,
        "the first names start a scan"
    );
    let cancel = cancel_flag(&mut opened);
    let created = opened.notes.update(&mut opened.visual, |notes, cx| {
        notes.create(None, Kind::Note, "plan c", cx)
    });
    opened.visual.run_until_parked();
    drop(created);
    assert!(
        !cancel.load(Ordering::Relaxed),
        "the second build waits for the scan"
    );

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(runs(&mut opened), before + 2, "one re-run");
    assert_eq!(listed(&mut opened), ["plan c", "f/plan b", "plan a"]);
}

#[gpui::test]
fn enter_during_a_dirty_scan_confirms_without_a_rerun(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "other.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    let plan = entry_id(&opened.notes, &mut opened.visual, "plan");
    let activated = Rc::new(RefCell::new(Vec::new()));
    let _guard = opened.visual.update(|_, cx| {
        let activated = Rc::clone(&activated);
        cx.subscribe(&opened.picker, move |_, event: &NoteActivated, _| {
            activated.borrow_mut().push(event.note);
        })
    });
    let release = hold_scans(&mut opened);
    opened.visual.simulate_keystrokes("p");
    notify(&mut opened);
    opened.visual.simulate_keystrokes("enter");
    let before = runs(&mut opened);

    let _ = release.send(());
    opened.visual.run_until_parked();

    assert_eq!(*activated.borrow(), [plan]);
    assert_eq!(
        runs(&mut opened),
        before,
        "the picker is dismissed: nothing to refresh"
    );
}
