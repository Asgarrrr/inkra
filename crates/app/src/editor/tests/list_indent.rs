//! `⇥` nests an item, `⇧⇥` moves it back up, each in one undo step; off a list, `⇥` is the plain
//! key and `⇧⇥` writes nothing (list editing spec, D4, D9).

use std::cell::Cell;
use std::rc::Rc;

use gpui::TestAppContext;

use super::{at, dispatching, editor, state};
use crate::editor::actions::{IndentItem, OutdentItem};

#[gpui::test]
fn tab_nests_an_item_and_shift_tab_moves_it_back(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- a\n- b", 7);
    cx.simulate_keystrokes("tab");
    assert_eq!(
        state(&window, &mut cx),
        ("- a\n  - b".to_owned(), 9..9, false)
    );

    cx.simulate_keystrokes("shift-tab");
    assert_eq!(
        state(&window, &mut cx),
        ("- a\n- b".to_owned(), 7..7, false)
    );
}

/// Code wants its tab; a selection is never replaced.
#[gpui::test]
fn off_a_list_tab_is_the_plain_key_and_shift_tab_writes_nothing(cx: &mut TestAppContext) {
    for (note, caret, keys, want) in [
        ("ab", 2, "tab", "ab\t"),
        (
            "- a\n  ```\n  x\n  ```",
            13,
            "tab",
            "- a\n  ```\n  x\t\n  ```",
        ),
        ("ab", 2, "shift-left tab", "ab"),
        ("ab", 2, "shift-tab", "ab"),
        ("ab", 2, "shift-left shift-tab", "ab"),
    ] {
        let (window, mut cx) = editor(cx, note, caret);
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&window, &mut cx).0, want, "{note:?} {keys:?}");
    }
}

/// A selection keeps its place: `apply_diff` maps it.
#[gpui::test]
fn tab_on_a_selection_moves_every_item_it_touches(cx: &mut TestAppContext) {
    let (window, mut cx) = editor(cx, "- a\n- b\n- c", 11);
    cx.simulate_keystrokes("up shift-down tab");
    assert_eq!(
        state(&window, &mut cx),
        ("- a\n  - b\n  - c".to_owned(), 9..15, false)
    );
}

/// Each row: the note, the caret, the key, the text and caret after it, and the caret after
/// `cmd-z`, as D9's table puts it: the end of the lowest hunk.
#[gpui::test]
fn cmd_z_undoes_tab_and_shift_tab_in_one_step(cx: &mut TestAppContext) {
    for (note, from, key, moved, landed, undone) in [
        ("- a\n- b", 7, "tab", "- a\n  - b", 9, 4),
        ("- a\n- b\n  c", 7, "tab", "- a\n  - b\n    c", 9, 10),
        ("- a\n  - b", 9, "shift-tab", "- a\n- b", 7, 6),
        // A restored marker's end sits inside the item part: the caret snaps past it.
        ("1. a\n   - b", 11, "shift-tab", "1. a\n2. b", 9, 9),
    ] {
        let (window, mut cx) = editor(cx, note, from);
        cx.simulate_keystrokes(key);
        let want = (moved.to_owned(), landed..landed, false);
        assert_eq!(state(&window, &mut cx), want, "{note:?} {key}");

        cx.simulate_keystrokes("cmd-z");
        let want = (note.to_owned(), undone..undone, false);
        assert_eq!(state(&window, &mut cx), want, "{note:?} {key}");
    }
}

/// A write that skips `refresh` leaves the parse behind the text: neither key edits, not even by
/// the plain `\t`.
#[gpui::test]
fn tab_and_shift_tab_on_a_stale_parse_edit_nothing(cx: &mut TestAppContext) {
    for (note, tab) in [("- a\n- b", true), ("- a\n  - b", false), ("ab", true)] {
        let (window, mut cx) = dispatching(cx, note);
        let end = note.len();
        window
            .update(&mut cx, |view, window, cx| {
                let buffer = view.buffer().clone();
                buffer.update(cx, |buffer, _| {
                    buffer.transact(|txn| txn.replace(at(end)..at(end), "!"));
                });
                if tab {
                    view.indent_item(&IndentItem, window, cx);
                } else {
                    view.outdent_item(&OutdentItem, window, cx);
                }
            })
            .unwrap();
        assert_eq!(state(&window, &mut cx).0, format!("{note}!"), "{note:?}");
    }
}

/// Each key reads the parse of the edit before it: a nested empty marker becomes an item once
/// typed into, and `enter` on an empty nested item moves it up a level (D3).
#[gpui::test]
fn keys_in_a_row_nest_and_unnest_a_new_item(cx: &mut TestAppContext) {
    for (note, keys, want) in [
        ("- a", "enter tab b enter", "- a\n  - b\n  - "),
        ("- a", "enter tab b enter enter", "- a\n  - b\n- "),
        ("- a", "enter tab shift-tab", "- a\n- "),
    ] {
        let (window, mut cx) = editor(cx, note, note.len());
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&window, &mut cx).0, want, "{note:?} {keys:?}");
    }
}

/// A lease notifies the buffer's observers, which would schedule an autosave for an edit that
/// writes nothing.
#[gpui::test]
fn a_key_that_moves_nothing_notifies_no_observer(cx: &mut TestAppContext) {
    for (note, keys) in [("- a", "tab"), ("- a", "shift-tab"), ("ab", "shift-tab")] {
        let (window, mut cx) = editor(cx, note, note.len());
        let notified = Rc::new(Cell::new(0));
        let count = Rc::clone(&notified);
        let _guard = window
            .update(&mut cx, |view, _, cx| {
                cx.observe(view.buffer(), move |_, _, _| count.set(count.get() + 1))
            })
            .unwrap();
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&window, &mut cx).0, note, "{note:?} {keys:?}");
        assert_eq!(notified.get(), 0, "{note:?} {keys:?}");
    }
}
