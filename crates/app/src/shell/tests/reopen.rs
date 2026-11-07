use gpui::{TestAppContext, px, size};

use super::{
    at, check, four_notes, keys, open, scroll_to, select, selected, showing, start_open, tab_files,
    tall_notes, trash, view_of,
};

#[gpui::test]
fn cmd_shift_t_reopens_the_closed_tab_at_its_index(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    keys(&mut cx, "cmd-2 cmd-w");
    assert_eq!(tab_files(&window, &mut cx).0, [Some(a), Some(c)]);

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(
        tab_files(&window, &mut cx),
        (vec![Some(a), Some(b), Some(c)], 1)
    );
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn the_last_closed_tab_reopens_first(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    keys(&mut cx, "cmd-2 cmd-w");
    assert_eq!(showing(&window, &mut cx), Some(c));
    keys(&mut cx, "cmd-w");

    keys(&mut cx, "cmd-shift-t");
    assert_eq!(showing(&window, &mut cx), Some(c));
    keys(&mut cx, "cmd-shift-t");

    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(
        tab_files(&window, &mut cx),
        (vec![Some(a), Some(b), Some(c)], 1)
    );
    check(&window, &mut cx);
}

/// Fails if the closed entry keeps `view: None`.
#[gpui::test]
fn a_reopened_tab_keeps_its_caret_and_top_line(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 610..612);
    let top = scroll_to(&window, &mut cx, 600);
    assert_ne!(top, at(0));
    keys(&mut cx, "cmd-t ctrl-tab cmd-w");
    assert_eq!(showing(&window, &mut cx), None);

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(view_of(&window, &mut cx), (at(610)..at(612), top));
    check(&window, &mut cx);
}

/// `close_tab` must unpark tab 1's caret and hand it to `resolve`, else the closed entry keeps no view.
#[gpui::test]
fn a_closed_tab_on_the_note_showing_reopens_with_the_view_its_anchors_moved_to(
    cx: &mut TestAppContext,
) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 610..610);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, 0..0);
    cx.simulate_input("new");
    keys(&mut cx, "enter");
    assert_eq!(view_of(&window, &mut cx).0, at(4)..at(4));
    window
        .update(&mut cx, |shell, window, cx| {
            let first = shell.center().tabs.id_at(0).unwrap();
            shell.close_tab(shell.layout.last_center(), first, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    check(&window, &mut cx);

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a), Some(a)], 0));
    assert_eq!(view_of(&window, &mut cx).0, at(614)..at(614));
    check(&window, &mut cx);
    keys(&mut cx, "ctrl-tab");
    assert_eq!(
        view_of(&window, &mut cx).0,
        at(4)..at(4),
        "parked by the reopen"
    );
    check(&window, &mut cx);
}

#[gpui::test]
fn a_reopened_tab_keeps_its_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t ctrl-tab cmd-w");

    keys(&mut cx, "cmd-shift-t");
    assert_eq!(showing(&window, &mut cx), Some(b));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    keys(&mut cx, "cmd-]");

    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn reopening_after_closing_the_last_tab_keeps_the_empty_tab(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-w");
    assert_eq!(tab_files(&window, &mut cx), (vec![None], 0));

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a), None], 0));
    check(&window, &mut cx);
}

#[gpui::test]
fn an_empty_tab_closed_with_no_history_is_skipped(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w cmd-t cmd-w");
    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a)], 0));

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_closed_tabs_note_with_no_history_drops_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w");

    trash(&window, &mut cx, b);
    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a)], 0));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_closed_tabs_note_keeps_its_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t ctrl-tab cmd-w");

    trash(&window, &mut cx, b);
    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx), (vec![None, None], 0));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_note_in_a_closed_tabs_history_drops_the_entry(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    keys(&mut cx, "cmd-t ctrl-tab cmd-w");

    trash(&window, &mut cx, b);
    keys(&mut cx, "cmd-shift-t");

    assert_eq!(showing(&window, &mut cx), Some(c));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

/// The reopened tab's fresh id admits no landing: only the held task shows whether `activation` dropped.
#[gpui::test]
fn reopen_cancels_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w");
    let gate = fs.block_load("/vault/c.md");
    start_open(&window, &mut cx, c);

    keys(&mut cx, "cmd-shift-t");
    assert_eq!(showing(&window, &mut cx), Some(b));
    let pending = window
        .update(&mut cx, |shell, _, _| shell.center().activation.is_some())
        .unwrap();
    assert!(!pending, "the reopen dropped the pending open");
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    check(&window, &mut cx);
}

/// T4: nothing changes on screen, so the pending open still lands.
#[gpui::test]
fn reopen_with_nothing_closed_keeps_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/b.md");
    start_open(&window, &mut cx, b);

    keys(&mut cx, "cmd-shift-t");
    assert_eq!(showing(&window, &mut cx), Some(a));
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

/// Catches a path that writes `shown` without `set_shown`.
#[gpui::test]
fn reopening_a_tab_selects_its_note_in_the_sidebar(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-1 cmd-w");
    assert_eq!(selected(&window, &mut cx), Some(b));

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(selected(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

/// 30 tabs cannot fit a 700 px window. Catches a missing `keep_active_tab_visible`.
#[gpui::test]
fn a_reopened_tab_scrolls_into_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        keys(&mut cx, "cmd-t");
    }
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w cmd-1");

    keys(&mut cx, "cmd-shift-t");

    assert_eq!(tab_files(&window, &mut cx).1, 29);
    let bar = cx.debug_bounds("tab-bar").expect("the bar is drawn");
    let reopened = cx.debug_bounds("tab-29").expect("the tab is drawn");
    assert!(
        reopened.left() >= bar.left() && reopened.right() <= bar.right(),
        "tab-29 at {reopened:?} is outside the bar at {bar:?}"
    );
    check(&window, &mut cx);
}

/// T6.
#[gpui::test]
fn cmd_shift_t_with_a_picker_open_dismisses_it_and_reopens(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-w cmd-p");

    keys(&mut cx, "cmd-shift-t");

    let modal = window
        .update(&mut cx, |shell, _, _| shell.modal.is_some())
        .unwrap();
    assert!(!modal, "the picker is dismissed");
    assert_eq!(tab_files(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    check(&window, &mut cx);
}
