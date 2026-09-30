use std::sync::Arc;

use futures::FutureExt as _;
use futures::channel::oneshot;
use gpui::TestAppContext;
use inkra_vault::Fs;

use super::{advance, empty_text, footer, listed, open, open_over, scans, search, with_texts};
use crate::index::BUILDING_REFRESH;
use crate::keymap::key_bindings;
use crate::picker::runs::SEARCH_DEBOUNCE;
use crate::test_support::unscanned_notes;

#[gpui::test]
fn an_empty_field_shows_nothing(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan")]);
    let mut opened = open(cx, &fs);

    assert_eq!(listed(&mut opened), Vec::<String>::new());
    assert_eq!(empty_text(&mut opened), None);

    search(&mut opened, "p backspace");
    assert_eq!(empty_text(&mut opened), None);
}

#[gpui::test]
fn blank_or_bracket_text_runs_no_lookup(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan [[b]]")]);
    let mut opened = open(cx, &fs);
    let before = scans(&mut opened);

    search(&mut opened, "space space space");
    assert_eq!(scans(&mut opened), before);
    assert_eq!(empty_text(&mut opened).as_deref(), Some("No matches"));

    search(&mut opened, "backspace backspace backspace");
    opened.visual.simulate_input("[[");
    advance(&mut opened, SEARCH_DEBOUNCE);
    assert_eq!(scans(&mut opened), before);
    assert_eq!(empty_text(&mut opened).as_deref(), Some("No matches"));
    assert_eq!(listed(&mut opened), Vec::<String>::new());
}

#[gpui::test]
fn a_query_with_no_match_says_so(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan")]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "z q");

    assert_eq!(listed(&mut opened), Vec::<String>::new());
    assert_eq!(empty_text(&mut opened).as_deref(), Some("No matches"));
    assert_eq!(footer(&mut opened), None);
}

#[gpui::test]
fn no_match_while_indexing_says_how_far(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "alpha"), ("b.md", "beta")]);
    let _held = fs.block_load("/vault/b.md");
    let mut opened = open(cx, &fs);

    search(&mut opened, "z q");

    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("No matches in 1 of 2 notes indexed so far")
    );
    assert_eq!(footer(&mut opened), None, "the empty text says it");
}

#[gpui::test]
fn no_match_while_indexing_counts_each_load_as_it_lands(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "alpha"), ("b.md", "beta"), ("c.md", "gamma")]);
    let b = fs.block_load("/vault/b.md");
    let _c = fs.block_load("/vault/c.md");
    let mut opened = open(cx, &fs);
    search(&mut opened, "z q");
    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("No matches in 1 of 3 notes indexed so far")
    );

    b.release();
    opened.visual.run_until_parked();

    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("No matches in 2 of 3 notes indexed so far"),
        "read when drawn, before the throttled refresh runs"
    );
}

#[gpui::test]
fn matches_while_indexing_say_so_under_the_rows(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan"), ("b.md", "plan")]);
    let _held = fs.block_load("/vault/b.md");
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    assert_eq!(listed(&mut opened), ["# a (1)", "1: plan"]);
    assert_eq!(
        footer(&mut opened).as_deref(),
        Some("Indexing 1 of 2 notes")
    );
}

#[gpui::test]
fn the_picker_says_scanning_then_lists_after_the_first_build(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = with_texts(&[("a.md", "plan"), ("f/b.md", "plan")]);
    // Held: typing runs until parked, which would otherwise finish the scan.
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut opened = open_over(cx, notes);

    opened.visual.simulate_keystrokes("p l a n");
    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("Scanning the vault…")
    );
    assert_eq!(scans(&mut opened), 0);

    gate.release();
    opened.visual.run_until_parked();
    advance(&mut opened, BUILDING_REFRESH);
    assert_eq!(
        listed(&mut opened),
        ["# b f(1)", "1: plan", "# a (1)", "1: plan"]
    );
}

#[gpui::test]
fn the_scanning_text_holds_until_the_first_rows_land(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = with_texts(&[("a.md", "plan"), ("f/b.md", "plan")]);
    let gate = fs.block_read_dir("/vault/f");
    // Keeps the index loading after the first build, so its refresh waits `BUILDING_REFRESH`.
    let _a = fs.block_load("/vault/a.md");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let mut opened = open_over(cx, notes);
    opened.visual.simulate_keystrokes("p l a n");

    gate.release();
    opened.visual.run_until_parked();
    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("Scanning the vault…"),
        "built, but no run has landed"
    );

    let (release, hold) = oneshot::channel::<()>();
    opened.picker.update(&mut opened.visual, |picker, _| {
        picker.delegate.runs.hold(Some(hold.shared()));
    });
    advance(&mut opened, BUILDING_REFRESH);
    assert_eq!(
        empty_text(&mut opened).as_deref(),
        Some("Scanning the vault…"),
        "a run started, but has not landed"
    );

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(listed(&mut opened), ["# b f(1)", "1: plan"]);
}
