//! `↩` continues a list, a quote or an item's text, leaves an empty item, and undoes in one step
//! (list editing spec, D2, D3, D9).

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_editor::Selection;

use super::{at, dispatching};
use crate::editor::actions::Newline;
use crate::editor::{EditorView, TopLine, ViewState};

type Handle = WindowHandle<EditorView>;

fn text(window: &Handle, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |view, _, cx| view.buffer().read(cx).text().to_owned())
        .unwrap()
}

fn caret(window: &Handle, cx: &mut VisualTestContext) -> usize {
    window.update(cx, |view, _, _| view.cursor().get()).unwrap()
}

fn place(window: &Handle, cx: &mut VisualTestContext, offset: usize) {
    window
        .update(cx, |view, _, cx| {
            let view_state = ViewState {
                selection: Selection::at(at(offset)),
                top: TopLine::at(at(0)),
                scrolls: Vec::new(),
            };
            view.restore(view_state, cx);
        })
        .unwrap();
}

#[gpui::test]
fn enter_continues_an_item_then_leaves_the_empty_one(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- a");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&window, &mut cx), "- a\n- ");
    assert_eq!(caret(&window, &mut cx), 6);

    cx.simulate_keystrokes("enter");
    assert_eq!(text(&window, &mut cx), "- a\n");
    assert_eq!(caret(&window, &mut cx), 4);
}

/// Each row: the note, the caret, the text and caret after `enter`, and the caret after `cmd-z`,
/// as D9's table puts it.
#[gpui::test]
fn cmd_z_undoes_enter_in_one_step(cx: &mut TestAppContext) {
    for (note, from, entered, landed, undone) in [
        // Rule 6, one hunk: where it was.
        ("- a", 3, "- a\n- ", 6, 3),
        // Rule 2, one hunk.
        ("- a\n  b", 7, "- a\n  b\n  ", 10, 7),
        // Rule 2 with a lazy-line prefix: the end of the lowest hunk, the lazy line's start.
        ("- a\n  b\nz", 7, "- a\n  b\n  \n  z", 10, 8),
        // Rule 3.
        ("- ```rust", 9, "- ```rust\n  ", 12, 9),
        // Rule 8.
        ("> a", 3, "> a\n> ", 6, 3),
        // Rule 6 with a width shift: the end of the lowest hunk, on a line below.
        ("9. a\n   - c", 4, "9. a\n10. \n    - c", 9, 8),
        // Rule 6 after blank lines: the new marker line's start.
        ("- a\n\n  b", 3, "- a\n\n- \n  b", 7, 4),
        // Rule 7, both forms: the marker line's start.
        ("- a\n- b", 6, "- a\n- \n- b", 9, 4),
        ("para\n- abc", 7, "para\n\n- abc", 8, 5),
        // D3 at the top level: where it was.
        ("- a\n- ", 6, "- a\n", 4, 6),
        // Rule 2 at the text start.
        ("- a\n  b", 6, "- a\n\n  b", 7, 6),
    ] {
        let (window, mut cx) = dispatching(cx, note);
        place(&window, &mut cx, from);
        cx.simulate_keystrokes("enter");
        assert_eq!(text(&window, &mut cx), entered, "{note:?} at {from}");
        assert_eq!(caret(&window, &mut cx), landed, "{note:?} at {from}");

        cx.simulate_keystrokes("cmd-z");
        assert_eq!(text(&window, &mut cx), note, "{note:?} at {from}");
        assert_eq!(caret(&window, &mut cx), undone, "{note:?} at {from}");
    }
}

/// The parse before the edit would snap the new caret against old atoms, to 13.
#[gpui::test]
fn the_caret_lands_at_the_new_lines_text_start(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- a\n  b \n  c");
    place(&window, &mut cx, 8);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&window, &mut cx), "- a\n  b\n  \n  c");
    assert_eq!(caret(&window, &mut cx), 10);
}

/// A plain `↩` still merges with the typing around it.
#[gpui::test]
fn enter_on_a_paragraph_inserts_a_newline_in_the_typing_run(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");
    cx.simulate_input("ab");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("cd");
    assert_eq!(text(&window, &mut cx), "ab\ncd");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&window, &mut cx), "");
}

/// A selection takes the plain key, even inside an item.
#[gpui::test]
fn enter_on_a_selection_replaces_it_with_a_newline(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- ab");
    cx.simulate_keystrokes("shift-left enter");
    assert_eq!(text(&window, &mut cx), "- a\n");
}

/// A write that skips `refresh` leaves the parse behind the text: the key edits nothing.
#[gpui::test]
fn enter_on_a_stale_parse_edits_nothing(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "- a");
    window
        .update(&mut cx, |view, window, cx| {
            let buffer = view.buffer().clone();
            buffer.update(cx, |buffer, _| {
                buffer.transact(|txn| txn.replace(at(3)..at(3), "!"));
            });
            view.newline(&Newline, window, cx);
        })
        .unwrap();
    assert_eq!(text(&window, &mut cx), "- a!");
}

/// Each key reads the parse of the edit before it, with no frame between them.
#[gpui::test]
fn keys_in_a_row_each_read_the_last_edits_parse(cx: &mut TestAppContext) {
    for (note, keys, want) in [
        ("- a", "enter enter", "- a\n"),
        ("1. a", "enter b enter", "1. a\n2. b\n3. "),
    ] {
        let (window, mut cx) = dispatching(cx, note);
        cx.simulate_keystrokes(keys);
        assert_eq!(text(&window, &mut cx), want, "{note:?} {keys:?}");
    }
}
