//! Hidden blanks are an atom (task boxes spec, D2): no caret rests inside one, and every move,
//! deletion and edit steps over it whole.

use std::cell::RefCell;
use std::ops::Range;

use gpui::{
    ClipboardItem, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle, point, px,
};

use inkra_text::Bias;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

use super::{at, close, dispatching};
use crate::editor::EditorView;
use crate::text_input::InputHost;

type Handle = WindowHandle<EditorView>;

/// A held line after a marker line: inside 4..6, edge 3, text start 6.
const HELD: &str = "- a\n  b\n\nbody";
/// The same, quoted: inside 6..10, edge 5, text start 10.
const QUOTED: &str = "> - a\n>   b\n\nbody";
/// A code line in a task: inside 15..17, edge 14, text start 17.
const CODE: &str = "- [ ] a\n\n  ```\n  x\n  ```\n\nbody";

/// The editor on `text`, drawn, its caret at `caret`.
fn editor(cx: &mut TestAppContext, text: &str, caret: usize) -> (Handle, VisualTestContext) {
    let (window, mut cx) = dispatching(cx, text);
    cx.run_until_parked();
    window
        .update(&mut cx, |view, window, cx| {
            let snap = view.snap();
            view.move_cursor(window, cx, |state, buffer| {
                state.move_to(buffer, at(caret), false, snap);
            });
        })
        .unwrap();
    (window, cx)
}

/// The text, the selection, and whether its head is its start.
fn state(window: &Handle, cx: &mut VisualTestContext) -> (String, Range<usize>, bool) {
    cx.run_until_parked();
    window
        .update(cx, |view, _, cx| {
            let selection = view.input.state().selection();
            let range = selection.start().get()..selection.end().get();
            let text = view.buffer().read(cx).text().to_owned();
            (text, range, selection.reversed())
        })
        .unwrap()
}

fn after(
    cx: &mut TestAppContext,
    text: &str,
    caret: usize,
    keys: &str,
) -> (String, Range<usize>, bool) {
    let (window, mut cx) = editor(cx, text, caret);
    cx.simulate_keystrokes(keys);
    state(&window, &mut cx)
}

#[gpui::test]
fn left_and_right_step_over_an_atom(cx: &mut TestAppContext) {
    for (text, edge, start) in [(HELD, 3, 6), (QUOTED, 5, 10), (CODE, 14, 17)] {
        for (from, keys, selection, reversed) in [
            (start, "left", edge..edge, false),
            (edge, "right", start..start, false),
            (start, "shift-left", edge..start, true),
            (edge, "shift-right", edge..start, false),
        ] {
            assert_eq!(
                after(cx, text, from, keys),
                (text.to_owned(), selection, reversed),
                "{text:?} {keys}"
            );
        }
    }
}

#[gpui::test]
fn backspace_and_delete_remove_an_atom_whole_and_undo_brings_it_back(cx: &mut TestAppContext) {
    for (text, edge, start, joined) in [
        (HELD, 3, 6, "- ab\n\nbody"),
        (QUOTED, 5, 10, "> - ab\n\nbody"),
        (CODE, 14, 17, "- [ ] a\n\n  ```x\n  ```\n\nbody"),
    ] {
        for (from, key) in [(start, "backspace"), (edge, "delete")] {
            let (window, mut cx) = editor(cx, text, from);
            cx.simulate_keystrokes(key);
            assert_eq!(
                state(&window, &mut cx),
                (joined.to_owned(), edge..edge, false),
                "{text:?} {key}"
            );
            cx.simulate_keystrokes("cmd-z");
            assert_eq!(
                state(&window, &mut cx),
                (text.to_owned(), start..start, false),
                "{text:?} {key} undone"
            );
        }
    }
}

#[gpui::test]
fn home_cmd_left_and_ctrl_a_land_at_a_held_lines_text(cx: &mut TestAppContext) {
    for (text, start, end) in [(HELD, 6, 7), (QUOTED, 10, 11), (CODE, 17, 18)] {
        for keys in ["home", "cmd-left", "ctrl-a"] {
            assert_eq!(
                after(cx, text, end, keys),
                (text.to_owned(), start..start, false),
                "{text:?} {keys}"
            );
        }
    }
}

#[gpui::test]
fn up_and_down_land_at_a_held_lines_text(cx: &mut TestAppContext) {
    // Line 2 is held: inside 6..8, its text at 8.
    let text = "x\n- a\n  b\n\nx";
    for (from, keys, selection, reversed) in [
        (0, "down down", 8..8, false),
        (11, "up up", 8..8, false),
        (0, "shift-down shift-down", 0..8, false),
        (11, "shift-up shift-up", 8..11, true),
    ] {
        assert_eq!(
            after(cx, text, from, keys),
            (text.to_owned(), selection, reversed),
            "{keys}"
        );
    }
}

/// A window point on row 1, left of its text, over the hidden blanks.
fn on_hidden_blanks(window: &Handle, cx: &mut VisualTestContext) -> Point<Pixels> {
    window
        .update(cx, |view, _, _| {
            let origin = view.text_bounds().unwrap().origin;
            let layout = view.input.last_layout().unwrap();
            origin + point(px(1.), layout.top_for_row(1) + px(14.))
        })
        .unwrap()
}

#[gpui::test]
fn a_click_and_a_drag_on_hidden_blanks_land_at_the_text(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, HELD, 0);
    let blanks = on_hidden_blanks(&window, &mut cx);
    cx.simulate_click(blanks, Modifiers::none());
    assert_eq!(state(&window, &mut cx), (HELD.to_owned(), 6..6, false));

    let from = window
        .update(&mut cx, |view, _, _| {
            let origin = view.text_bounds().unwrap().origin;
            let layout = view.input.last_layout().unwrap();
            origin + point(layout.x_for_offset(at(2)), layout.top_for_row(0) + px(14.))
        })
        .unwrap();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    let away = from + point(px(0.), px(8.));
    cx.simulate_mouse_move(away, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(blanks, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(blanks, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&window, &mut cx), (HELD.to_owned(), 2..6, false));
}

/// An edit at either edge keeps the caret out of an atom, one the edit makes included.
#[gpui::test]
fn enter_typing_and_paste_at_an_atoms_edges(cx: &mut TestAppContext) {
    for (text, from, keys, edited, caret) in [
        ("- a  b", 3, "enter", "- a\n  b", 6),
        (HELD, 3, "enter", "- a\n\n  b\n\nbody", 4),
        (HELD, 6, "enter", "- a\n  \nb\n\nbody", 7),
        (HELD, 3, "x", "- ax\n  b\n\nbody", 4),
        (HELD, 6, "x", "- a\n  xb\n\nbody", 7),
        ("- a  b", 3, "cmd-v", "- a\n- c\n  b", 10),
        (HELD, 3, "cmd-v", "- a\n- c\n\n  b\n\nbody", 8),
        (HELD, 6, "cmd-v", "- a\n  \n- c\nb\n\nbody", 11),
        ("- a\n- b", 4, "delete", "- a\n b", 5),
    ] {
        let (window, mut cx) = editor(cx, text, from);
        cx.write_to_clipboard(ClipboardItem::new_string("\n- c\n".to_owned()));
        cx.simulate_keystrokes(keys);
        assert_eq!(
            state(&window, &mut cx),
            (edited.to_owned(), caret..caret, false),
            "{text:?} at {from}: {keys}"
        );
    }
}

const TOKENS: &[&str] = &[
    "- ", "- [ ] ", "1. ", "> ", "  ", "    ", "\t", "a", "a  ", "\n", "\n- a", "\n\n", "```",
];

const KEYS: &[&str] = &[
    "left",
    "right",
    "shift-left",
    "shift-right",
    "backspace",
    "delete",
    "home",
    "end",
    "ctrl-a",
    "ctrl-e",
    "up",
    "down",
    "shift-up",
    "shift-down",
    "cmd-up",
    "cmd-down",
    "enter",
    "x",
    "space",
    "tab",
    "cmd-v",
    "cmd-z",
    "cmd-shift-z",
];

/// After every key, neither end of the selection lies inside an atom of the text as it stands.
fn check(
    cx: &mut TestAppContext,
    text: &str,
    caret: usize,
    keys: &[&str],
) -> Result<(), TestCaseError> {
    let (window, mut cx) = editor(cx, text, caret);
    cx.write_to_clipboard(ClipboardItem::new_string("\n- c\n  ".to_owned()));
    for key in keys {
        cx.simulate_keystrokes(key);
        let (text, selection, _) = state(&window, &mut cx);
        let markdown = inkra_markdown::parse(&text);
        for end in [selection.start, selection.end] {
            let rests = markdown.snap(at(end), Bias::Left) == at(end);
            prop_assert!(rests, "{text:?}: {selection:?} after {key}");
        }
    }
    close(&mut cx);
    Ok(())
}

#[gpui::test]
fn no_key_leaves_an_end_of_the_selection_inside_an_atom(cx: &mut TestAppContext) {
    let cx = RefCell::new(cx);
    let config = Config {
        cases: 512,
        source_file: Some(file!()),
        ..Config::default()
    };
    let text = prop::collection::vec(prop::sample::select(TOKENS), 0..12)
        .prop_map(|tokens| tokens.concat());
    let keys = prop::collection::vec(prop::sample::select(KEYS), 1..10);
    let strategy = (text, any::<prop::sample::Index>(), keys);
    let result = TestRunner::new(config).run(&strategy, |(text, caret, keys)| {
        check(
            &mut cx.borrow_mut(),
            &text,
            caret.index(text.len() + 1),
            &keys,
        )
    });
    if let Err(failure) = result {
        panic!("{failure}");
    }
}
