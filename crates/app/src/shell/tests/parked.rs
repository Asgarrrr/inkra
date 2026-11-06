//! An inactive tab's view kept as anchors: edits, undo, reload or trash elsewhere move or drop it (tabs spec, A2).

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_editor::Selection;
use inkra_text::ByteOffset;
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::{
    ab_focused, at, check, editor, open, scroll_to, select, shell_focused, showing, tab_notes,
    tall_notes, text_of, trash,
};
use crate::shell::AppShell;
use crate::test_support;

fn selection(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Selection {
    editor(window, cx).update(cx, |editor, _| editor.view_state().selection)
}

fn caret(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> ByteOffset {
    editor(window, cx).update(cx, |editor, _| editor.cursor())
}

fn top(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> ByteOffset {
    editor(window, cx).update(cx, |editor, _| editor.offset_at_top())
}

#[gpui::test]
fn an_edit_and_its_undo_in_another_tab_move_a_parked_caret_with_its_text(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input("xy");

    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(caret(&window, &mut cx), at(5));
    check(&window, &mut cx);

    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text_of(&window, &mut cx, a), "alpha");
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(caret(&window, &mut cx), at(3));
    check(&window, &mut cx);
}

/// The selection keeps the text left of it, not an end.
#[gpui::test]
fn a_parked_selection_partly_deleted_in_another_tab_keeps_its_remaining_text(
    cx: &mut TestAppContext,
) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 4..4);
    cx.simulate_keystrokes("shift-left shift-left shift-left");
    let parked = selection(&window, &mut cx);
    assert_eq!((parked.range(), parked.reversed()), (at(1)..at(4), true));
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, 0..2);
    cx.simulate_keystrokes("backspace");
    assert_eq!(text_of(&window, &mut cx, a), "pha");

    cx.simulate_keystrokes("ctrl-tab");

    let back = selection(&window, &mut cx);
    assert_eq!((back.range(), back.reversed()), (at(0)..at(2), true));
}

/// `line\n` is five bytes, `new\n` four: line 120 starts at 600, then at 620.
#[gpui::test]
fn a_parked_top_line_follows_lines_inserted_above_it(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    open(&window, &mut cx, a);
    assert_eq!(scroll_to(&window, &mut cx, 600), at(600));
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input(&"new\n".repeat(5));

    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();

    assert_eq!(top(&window, &mut cx), at(620));
}

#[gpui::test]
fn a_switch_with_no_edit_keeps_a_reversed_selection_and_its_goal(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 612..612);
    cx.simulate_keystrokes("shift-up shift-up");
    let parked = selection(&window, &mut cx);
    assert!(parked.reversed());
    assert!(matches!(parked.goal(), inkra_editor::Goal::Column(_)));

    cx.simulate_keystrokes("cmd-t");
    cx.simulate_keystrokes("ctrl-tab");

    assert_eq!(selection(&window, &mut cx), parked);
}

#[gpui::test]
fn closing_the_active_tab_shows_the_next_tab_with_its_moved_caret(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input("xy");

    cx.simulate_keystrokes("cmd-w");

    assert_eq!(caret(&window, &mut cx), at(5));
    check(&window, &mut cx);
}

#[gpui::test]
fn closing_an_inactive_tab_unparks_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    window
        .update(&mut cx, |shell, window, cx| {
            let inactive = shell.center().tabs.id_at(0).unwrap();
            shell.close_tab(inactive, window, cx);
        })
        .unwrap();

    check(&window, &mut cx);
}

const A: &str = "/vault/a.md";

fn numbered() -> String {
    (0..200).fold(String::new(), |mut text, n| {
        let _ = writeln!(text, "line {n}");
        text
    })
}

/// `a` numbered, `b` short, the shell focused.
fn numbered_notes(
    cx: &mut TestAppContext,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    EntryId,
    EntryId,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, numbered());
    fs.insert_file("/vault/b.md", "beta");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    (fs, window, cx, a, b)
}

fn line_at(window: &WindowHandle<AppShell>, cx: &mut TestAppContext, offset: ByteOffset) -> String {
    let note = showing(window, cx).unwrap();
    let text = text_of(window, cx, note);
    text[offset.get()..]
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn offset_of(line: &str) -> usize {
    numbered().find(&format!("{line}\n")).unwrap()
}

/// Through the watcher, as `reload.rs`: the file changes on disk while `a` is parked.
#[gpui::test]
fn a_reload_moves_a_parked_caret_and_top_line_with_their_text(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = numbered_notes(cx);
    open(&window, &mut cx, a);
    let caret_at = offset_of("line 120");
    select(&window, &mut cx, caret_at..caret_at);
    let top_at = scroll_to(&window, &mut cx, offset_of("line 100"));
    assert_eq!(line_at(&window, &mut cx, top_at), "line 100");
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    fs.insert_file(A, format!("{}{}", "new\n".repeat(5), numbered()));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();

    let (caret, top) = (caret(&window, &mut cx), top(&window, &mut cx));
    assert_eq!(caret, at(caret_at + 20));
    assert_eq!(line_at(&window, &mut cx, caret), "line 120");
    assert_eq!(line_at(&window, &mut cx, top), "line 100");
}

/// Different carets, so a tab coming back with the other's view fails too.
#[gpui::test]
fn a_reload_under_two_tabs_on_one_note_moves_both_views(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = numbered_notes(cx);
    let (parked_at, active_at) = (offset_of("line 120"), offset_of("line 50"));
    open(&window, &mut cx, a);
    select(&window, &mut cx, parked_at..parked_at);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, active_at..active_at);

    fs.insert_file(A, format!("{}{}", "new\n".repeat(5), numbered()));
    cx.run_until_parked();
    let active = caret(&window, &mut cx);
    cx.simulate_keystrokes("ctrl-tab");
    let parked = caret(&window, &mut cx);

    assert_eq!((active, parked), (at(active_at + 20), at(parked_at + 20)));
    assert_eq!(line_at(&window, &mut cx, parked), "line 120");
    check(&window, &mut cx);
}

/// As `gone.rs`: `a` leaves the disk, shows its banner, and comes back while parked.
#[gpui::test]
fn a_gone_then_restored_note_moves_its_parked_view(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = numbered_notes(cx);
    let caret_at = offset_of("line 120");
    open(&window, &mut cx, a);
    select(&window, &mut cx, caret_at..caret_at);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    fs.remove(Path::new(A));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();
    assert!(cx.debug_bounds("gone-banner").is_some(), "a is gone");
    assert_eq!(caret(&window, &mut cx), at(caret_at));
    cx.simulate_keystrokes("ctrl-tab");

    fs.insert_file(A, format!("{}{}", "new\n".repeat(5), numbered()));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();

    assert!(cx.debug_bounds("gone-banner").is_none(), "a is back");
    let caret = caret(&window, &mut cx);
    assert_eq!(caret, at(caret_at + 20));
    assert_eq!(line_at(&window, &mut cx, caret), "line 120");
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_parked_note_drops_its_entry(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    trash(&window, &mut cx, a);
    check(&window, &mut cx);
    cx.simulate_keystrokes("ctrl-tab");

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, Some(b)], 0));
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_note_parked_in_two_tabs_and_showing_in_a_third(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    for _ in 0..2 {
        open(&window, &mut cx, a);
        cx.simulate_keystrokes("cmd-t");
    }
    open(&window, &mut cx, a);

    trash(&window, &mut cx, a);

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, None, None], 2));
    check(&window, &mut cx);
    window
        .update(&mut cx, |shell, window, _| {
            assert!(
                shell.focus_handle.is_focused(window),
                "the root keeps the keys"
            );
        })
        .unwrap();
}
