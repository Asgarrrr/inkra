use gpui::{Focusable, TestAppContext};

use super::{
    at, check, click_row, editor, focus_tree, four_notes, keys, open, scroll_to, select, showing,
    start_open, tall_notes, text_of, trash, view_of,
};
use crate::open::Open;

#[gpui::test]
fn back_and_forward_restore_each_notes_caret_and_top_line(cx: &mut TestAppContext) {
    let (window, mut cx, a, b) = tall_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 610..612);
    let a_top = scroll_to(&window, &mut cx, 600);
    open(&window, &mut cx, b);
    select(&window, &mut cx, 205..205);
    let b_top = scroll_to(&window, &mut cx, 200);
    assert_ne!(a_top, b_top);

    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(view_of(&window, &mut cx), (at(610)..at(612), a_top));
    check(&window, &mut cx);

    keys(&mut cx, "cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(view_of(&window, &mut cx), (at(205)..at(205), b_top));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_new_open_clears_forward(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    open(&window, &mut cx, c);

    keys(&mut cx, "cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(c), "forward was cleared");
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn each_tab_keeps_its_own_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    open(&window, &mut cx, d);

    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(c));
    keys(&mut cx, "ctrl-tab");
    assert_eq!(showing(&window, &mut cx), Some(b));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

/// Tab 1 holds `a` behind and `b` ahead: a switch that pushed onto the tab left would clear
/// forward, and one that pushed onto the tab arriving would put `c` behind `a`.
#[gpui::test]
fn a_tab_switch_pushes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-[");
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);

    keys(&mut cx, "ctrl-tab");
    assert_eq!(showing(&window, &mut cx), Some(a));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a), "nothing behind `a`");
    keys(&mut cx, "cmd-]");

    assert_eq!(showing(&window, &mut cx), Some(b), "forward kept");
    check(&window, &mut cx);
}

#[gpui::test]
fn back_cancels_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    let gate = fs.block_load("/vault/c.md");
    start_open(&window, &mut cx, c);

    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn back_with_nothing_behind_keeps_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/b.md");
    start_open(&window, &mut cx, b);

    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn back_seals_the_undo_group_of_the_note_left(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    cx.simulate_input("x");

    keys(&mut cx, "cmd-[ cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(b));
    cx.simulate_input("y");
    keys(&mut cx, "cmd-z");

    assert_eq!(text_of(&window, &mut cx, b), "xbeta");
    check(&window, &mut cx);
}

/// Entries keep plain offsets (A2): an edit elsewhere is not tracked, only clipped.
#[gpui::test]
fn a_history_entry_whose_text_shrank_clips_on_return(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 5..5);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, 1..5);
    keys(&mut cx, "backspace");
    assert_eq!(text_of(&window, &mut cx, a), "a");

    keys(&mut cx, "ctrl-tab cmd-[");

    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(view_of(&window, &mut cx).0, at(1)..at(1));
    check(&window, &mut cx);
}

/// A push would clear forward, so `⌘]` would stay on `a`.
#[gpui::test]
fn a_jump_inside_the_note_showing_pushes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-[");
    // Outside a window update, as the hit's subscription runs: `show_note` needs `with_window`.
    let shell = window.root(&mut cx).unwrap();
    shell.update(&mut cx, |shell, cx| {
        shell.open(a, Some(at(1)..at(2)), Open::Here, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        editor(&window, &mut cx).update(&mut cx, |editor, _| editor.selected()),
        at(1)..at(2)
    );

    keys(&mut cx, "cmd-]");

    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_note_drops_it_from_every_tabs_history(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    open(&window, &mut cx, d);

    trash(&window, &mut cx, b);

    keys(&mut cx, "cmd-[");
    assert_eq!(
        showing(&window, &mut cx),
        Some(d),
        "tab 2 had only b behind"
    );
    keys(&mut cx, "ctrl-tab");
    assert_eq!(showing(&window, &mut cx), Some(c));
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_note_drops_it_from_forward_too(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = four_notes(cx);
    for note in [a, b, c] {
        open(&window, &mut cx, note);
    }
    keys(&mut cx, "cmd-[ cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));

    trash(&window, &mut cx, b);
    keys(&mut cx, "cmd-]");

    assert_eq!(showing(&window, &mut cx), Some(c));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_tab_emptied_by_a_trash_goes_back_to_its_last_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    trash(&window, &mut cx, b);
    assert_eq!(showing(&window, &mut cx), None);

    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    keys(&mut cx, "cmd-]");

    assert_eq!(
        showing(&window, &mut cx),
        Some(a),
        "the empty place left was not pushed"
    );
    check(&window, &mut cx);
}

#[gpui::test]
fn a_sidebar_click_on_the_note_showing_keeps_forward(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    click_row(&mut cx, 0.);
    click_row(&mut cx, 1.);
    keys(&mut cx, "cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));

    click_row(&mut cx, 0.);
    keys(&mut cx, "cmd-]");

    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_sidebar_click_on_the_note_showing_focuses_the_editor(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, ..]) = four_notes(cx);
    click_row(&mut cx, 0.);
    focus_tree(&window, &mut cx);

    click_row(&mut cx, 0.);

    assert_eq!(showing(&window, &mut cx), Some(a));
    let focused = window
        .update(&mut cx, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .unwrap();
    assert!(focused, "the editor takes the keys");
    check(&window, &mut cx);
}
