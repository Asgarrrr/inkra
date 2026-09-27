use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt as _;
use futures::channel::oneshot;
use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px};

use super::refresh::{advance, hold_scans};
use super::{Opened, SEARCH_DEBOUNCE, listed, open, search, with_texts};
use crate::picker::PickerDelegate as _;
use crate::theme;

/// Holds every excerpt asked for from now on, once computed, until the sender is dropped or sent.
fn hold_excerpts(opened: &mut Opened) -> oneshot::Sender<()> {
    let (release, hold) = oneshot::channel();
    opened.picker.update(&mut opened.visual, |picker, _| {
        picker.delegate.hold_excerpts = Some(hold.shared());
    });
    release
}

fn asked(opened: &mut Opened) -> Vec<usize> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        let rows = picker.delegate.list.rows.iter().enumerate();
        rows.filter(|(_, row)| row.asked)
            .map(|(ix, _)| ix)
            .collect()
    })
}

fn second_lines(opened: &mut Opened) -> Vec<Option<String>> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        let rows = picker.delegate.list.rows.iter();
        rows.map(|row| row.excerpt.as_ref().map(|excerpt| excerpt.text.clone()))
            .collect()
    })
}

fn excerpt_cancel(opened: &mut Opened) -> Arc<AtomicBool> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        Arc::clone(&picker.delegate.list.cancel)
    })
}

fn thirty_plans() -> Vec<(String, String)> {
    (0..30)
        .map(|n| (format!("plan {n:02}.md"), format!("Plan number {n:02}.")))
        .collect()
}

fn open_thirty(cx: &mut TestAppContext) -> Opened {
    let files = thirty_plans();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    open(cx, &with_texts(&files), None, &[])
}

#[gpui::test]
fn rows_land_before_their_excerpts(cx: &mut TestAppContext) {
    let fs = with_texts(&[("travel.md", "We plan the trip."), ("trip.md", "Rome.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let release = hold_excerpts(&mut opened);

    search(&mut opened, "t r i p");
    assert_eq!(listed(&mut opened), ["trip", "travel"]);
    assert_eq!(second_lines(&mut opened), [None, None]);
    let label = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.row_label(1));
    assert_eq!(label.as_ref(), "travel", "the path alone until the excerpt");

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(
        second_lines(&mut opened),
        [
            Some("Rome.".to_owned()),
            Some("We plan the trip.".to_owned())
        ]
    );
}

#[gpui::test]
fn a_name_row_shows_its_first_prose_line(cx: &mut TestAppContext) {
    let text = "---\ntags: [a]\n---\n# Plan\n\n- **Ship** the `picker`.\n";
    let fs = with_texts(&[("plan.md", text)]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");

    assert_eq!(
        second_lines(&mut opened),
        [Some("Ship the picker.".to_owned())]
    );
}

#[gpui::test]
fn only_the_rows_on_screen_get_excerpts(cx: &mut TestAppContext) {
    let mut opened = open_thirty(cx);

    search(&mut opened, "p l a n");

    assert_eq!(listed(&mut opened).len(), 30);
    assert_eq!(asked(&mut opened), (0..10).collect::<Vec<_>>());
    let lines = second_lines(&mut opened);
    assert!(lines[..10].iter().all(Option::is_some));
    assert!(lines[10..].iter().all(Option::is_none));
}

#[gpui::test]
fn scrolling_asks_for_the_rows_it_shows(cx: &mut TestAppContext) {
    let mut opened = open_thirty(cx);
    search(&mut opened, "p l a n");
    let list = opened.visual.debug_bounds("picker-list");
    assert!(list.is_some(), "the list is drawn");

    opened.visual.simulate_event(ScrollWheelEvent {
        position: list.map(|list| list.center()).unwrap_or_default(),
        delta: ScrollDelta::Pixels(point(px(0.), -(theme::UI_LINE_HEIGHT * 2. * 20.))),
        modifiers: Modifiers::none(),
        ..ScrollWheelEvent::default()
    });
    opened.visual.run_until_parked();

    assert!(opened.visual.debug_bounds("picker-row-29").is_some());
    assert_eq!(
        asked(&mut opened),
        (0..10).chain(20..30).collect::<Vec<_>>()
    );
    let lines = second_lines(&mut opened);
    assert!(lines[20..].iter().all(Option::is_some));
    assert!(lines[10..20].iter().all(Option::is_none));
}

#[gpui::test]
fn an_excerpt_for_an_older_query_never_lands(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Ship it."), ("trip.md", "A plan for Rome.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let excerpts = hold_excerpts(&mut opened);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened), ["plan", "trip"]);
    let scans = hold_scans(&mut opened);

    opened.visual.simulate_keystrokes("space r");
    let _ = excerpts.send(());
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["plan", "trip"], "the scan is held");
    assert_eq!(second_lines(&mut opened), [None, None]);

    let _ = scans.send(());
    // `r` found the scan of `space` running: it waits the debounce.
    advance(&mut opened, SEARCH_DEBOUNCE);
    assert_eq!(listed(&mut opened), ["trip"]);
    let lit = opened.picker.read_with(&opened.visual, |picker, _| {
        let rows = &picker.delegate.list.rows;
        rows.iter()
            .map(|row| row.excerpt.as_ref().map(|excerpt| excerpt.highlights.len()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        lit,
        [Some(2)],
        "`plan` and `Rome` lit: the new query's excerpt"
    );
}

#[gpui::test]
fn an_excerpt_for_a_replaced_list_never_lands(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Ship it to Rome.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let first = hold_excerpts(&mut opened);
    search(&mut opened, "p l a n");
    let second = hold_excerpts(&mut opened);
    search(&mut opened, "space r o m e");
    assert_eq!(listed(&mut opened), ["plan"]);

    let _ = second.send(());
    opened.visual.run_until_parked();
    let _ = first.send(());
    opened.visual.run_until_parked();

    let lit = opened.picker.read_with(&opened.visual, |picker, _| {
        let rows = &picker.delegate.list.rows;
        rows.iter()
            .map(|row| row.excerpt.as_ref().map(|excerpt| excerpt.highlights.len()))
            .collect::<Vec<_>>()
    });
    assert_eq!(lit, [Some(1)], "`Rome` lit, not `plan`'s unlit first line");
}

#[gpui::test]
fn a_new_query_cancels_the_excerpts_in_flight(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Ship it.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let _release = hold_excerpts(&mut opened);
    search(&mut opened, "p l a n");
    let cancel = excerpt_cancel(&mut opened);
    assert!(!cancel.load(Ordering::Relaxed));
    // Held, so the flag is read before the next list lands, which would set it too.
    let _scans = hold_scans(&mut opened);

    opened.visual.simulate_keystrokes("backspace");

    assert_eq!(listed(&mut opened), ["plan"], "the old list is still shown");
    assert!(cancel.load(Ordering::Relaxed));
}

#[gpui::test]
fn dismissing_the_picker_cancels_its_excerpts(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Ship it.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let _release = hold_excerpts(&mut opened);
    search(&mut opened, "p l a n");
    let cancel = excerpt_cancel(&mut opened);

    opened.visual.update(|window, _| window.remove_window());
    drop(opened.picker);
    // gpui releases a dropped entity when effects flush, and the window is gone.
    opened.visual.cx.update(|_| {});

    assert!(cancel.load(Ordering::Relaxed));
}

#[gpui::test]
fn the_second_line_fills_without_the_row_moving(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan a.md", "Ship it."), ("plan b.md", "Then rest.")]);
    let mut opened = open(cx, &fs, None, &[]);
    let release = hold_excerpts(&mut opened);
    search(&mut opened, "p l a n");
    let rows = |opened: &mut Opened| {
        ["picker-row-0", "picker-row-1"].map(|row| opened.visual.debug_bounds(row))
    };
    let before = rows(&mut opened);
    assert!(before.iter().all(Option::is_some), "both rows are drawn");

    let _ = release.send(());
    opened.visual.run_until_parked();

    assert_eq!(
        second_lines(&mut opened),
        [Some("Then rest.".to_owned()), Some("Ship it.".to_owned())]
    );
    assert_eq!(rows(&mut opened), before);
}

#[gpui::test]
fn a_refresh_shows_the_previous_excerpt_until_its_own_lands(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("plan a.md", "Ship it."),
        ("plan b.md", "Then rest."),
        ("other.md", ""),
    ]);
    let gate = fs.block_load("/vault/plan b.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened), ["plan b", "plan a"]);
    assert_eq!(
        second_lines(&mut opened),
        [None, Some("Ship it.".to_owned())],
        "plan b is not loaded"
    );
    let release = hold_excerpts(&mut opened);
    let replaced = excerpt_cancel(&mut opened);

    gate.release();
    opened.visual.run_until_parked();
    assert!(replaced.load(Ordering::Relaxed), "the next list cancels");
    assert_eq!(
        second_lines(&mut opened),
        [None, Some("Ship it.".to_owned())],
        "the refresh landed; its excerpts are held"
    );
    assert_eq!(
        asked(&mut opened),
        [0, 1],
        "asked again: a text may have changed"
    );

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(
        second_lines(&mut opened),
        [Some("Then rest.".to_owned()), Some("Ship it.".to_owned())],
        "the loaded note gets its second line"
    );
}
