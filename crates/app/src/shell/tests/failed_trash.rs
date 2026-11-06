use std::sync::Arc;

use gpui::{Focusable, Task, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::{Gate, MemFs};

use super::{
    AppShell, ab_focused, at, check, editor, fail_trash, finish_trash, focus_tree, open, scroll_to,
    select, shell_focused, showing, start_open, start_trash, tab_notes, text_of, view_of,
};
use crate::open::Open;
use crate::test_support;

/// Tabs `[b, a]` with `b` active; `a` was typed into, its save is held at `gate`, and its trash waits behind it.
struct Pending {
    fs: Arc<MemFs>,
    window: WindowHandle<AppShell>,
    cx: VisualTestContext,
    a: EntryId,
    b: EntryId,
    c: EntryId,
    gate: Gate,
    trashing: Task<bool>,
}

/// `a` "alpha", `b` "beta", `c` "gamma"; `untrashable` makes `a`'s trash fail.
fn pending_trash(cx: &mut TestAppContext, untrashable: bool) -> Pending {
    let fs = Arc::new(MemFs::new());
    for (name, text) in [("a", "alpha"), ("b", "beta"), ("c", "gamma")] {
        fs.insert_file(format!("/vault/{name}.md"), text);
    }
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let [a, b, c] = ["a", "b", "c"].map(|name| test_support::entry_id(&notes, &mut cx, name));
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    cx.simulate_keystrokes("ctrl-tab");
    if untrashable {
        fs.make_untrashable("/vault/a.md");
    }
    let trashing = start_trash(&window, &mut cx, a);
    Pending {
        fs,
        window,
        cx,
        a,
        b,
        c,
        gate,
        trashing,
    }
}

fn root_has_focus(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, _| shell.focus_handle.is_focused(window))
        .unwrap()
}

fn tree_has_focus(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .unwrap()
}

fn editor_has_focus(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            let editor = shell.editor().cloned().expect("a note is bound");
            editor.focus_handle(cx).is_focused(window)
        })
        .unwrap()
}

#[gpui::test]
fn a_failed_trash_after_a_switch_reached_its_note_shows_it_again(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        a,
        b,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(showing(&window, &mut cx), None, "no editor to type into");

    fail_trash(gate, trashing, &mut cx);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(a)], 1));
    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(text_of(&window, &mut cx, a), "alphax");
    check(&window, &mut cx);
}

#[gpui::test]
fn a_failed_trash_keeps_its_note_in_inactive_tabs_and_history(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("ctrl-tab");
    fs.make_untrashable("/vault/a.md");
    let trashing = start_trash(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-[");
    assert_eq!(showing(&window, &mut cx), None);

    fail_trash(gate, trashing, &mut cx);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(a)], 0));
    assert_eq!(showing(&window, &mut cx), Some(a));
    cx.simulate_keystrokes("cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn back_from_a_trashing_tab_skips_its_note(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, b);
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    // Back is now `[b, a]`: the helper's open does not skip the note showing.
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    fs.make_untrashable("/vault/a.md");
    let trashing = start_trash(&window, &mut cx, a);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), None], 0));

    cx.simulate_keystrokes("cmd-[");

    assert_eq!(showing(&window, &mut cx), Some(b));
    fail_trash(gate, trashing, &mut cx);
    check(&window, &mut cx);
}

#[gpui::test]
fn closing_onto_a_trashing_tab_shows_it_empty_then_again(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        a,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);

    cx.simulate_keystrokes("cmd-w");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
    fail_trash(gate, trashing, &mut cx);
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_new_tab_from_a_trashing_tab_keeps_it(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        a,
        b,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");

    window
        .update(&mut cx, |shell, _, cx| {
            shell.open(b, None, Open::NewTab, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(b), Some(a), Some(b)], 2)
    );
    check(&window, &mut cx);
    fail_trash(gate, trashing, &mut cx);

    cx.simulate_keystrokes("ctrl-shift-tab");

    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

/// A trash outcome is not a user action: the open started from the `Trashing` tab still lands (T4).
#[gpui::test]
fn an_open_from_a_trashing_tab_lands_after_the_trash_fails(cx: &mut TestAppContext) {
    let Pending {
        fs,
        window,
        mut cx,
        a,
        b,
        c,
        gate,
        trashing,
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    let load = fs.block_load("/vault/c.md");
    start_open(&window, &mut cx, c);

    fail_trash(gate, trashing, &mut cx);
    load.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(c)], 1));
    assert_eq!(showing(&window, &mut cx), Some(c));
    cx.simulate_keystrokes("cmd-[");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_new_tab_open_from_a_trashing_tab_lands_after_the_trash_fails(cx: &mut TestAppContext) {
    let Pending {
        fs,
        window,
        mut cx,
        a,
        b,
        c,
        gate,
        trashing,
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    let load = fs.block_load("/vault/c.md");
    window
        .update(&mut cx, |shell, _, cx| {
            shell.open(c, None, Open::NewTab, cx);
        })
        .unwrap();
    cx.run_until_parked();

    fail_trash(gate, trashing, &mut cx);
    load.release();
    cx.run_until_parked();

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(b), Some(a), Some(c)], 2)
    );
    assert_eq!(showing(&window, &mut cx), Some(c));
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(showing(&window, &mut cx), Some(a));
    check(&window, &mut cx);
}

#[gpui::test]
fn an_open_from_a_trashing_tab_lands_after_the_trash_succeeds(cx: &mut TestAppContext) {
    let Pending {
        fs,
        window,
        mut cx,
        b,
        c,
        gate,
        trashing,
        ..
    } = pending_trash(cx, false);
    cx.simulate_keystrokes("ctrl-tab");
    let load = fs.block_load("/vault/c.md");
    start_open(&window, &mut cx, c);

    finish_trash(&fs, gate, trashing, &mut cx);
    load.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(c)], 1));
    assert_eq!(showing(&window, &mut cx), Some(c));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_failed_trash_gives_the_editor_the_focus_the_root_held(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    assert!(root_has_focus(&window, &mut cx));

    fail_trash(gate, trashing, &mut cx);

    assert!(editor_has_focus(&window, &mut cx));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_failed_trash_from_the_tree_keeps_the_tree_focused(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let view = editor(&window, &mut cx).entity_id();
    focus_tree(&window, &mut cx);
    fs.make_untrashable("/vault/a.md");

    let trashing = start_trash(&window, &mut cx, a);

    assert!(!futures::executor::block_on(trashing), "the trash failed");
    assert!(tree_has_focus(&window, &mut cx));
    assert_eq!(editor(&window, &mut cx).entity_id(), view);
    check(&window, &mut cx);
}

/// The focus predicate's other side: the tree took the focus from the root while the tab was empty.
#[gpui::test]
fn a_failed_trash_leaves_the_focus_on_the_tree(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        a,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    focus_tree(&window, &mut cx);

    fail_trash(gate, trashing, &mut cx);

    assert_eq!(showing(&window, &mut cx), Some(a));
    assert!(tree_has_focus(&window, &mut cx));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_picker_opened_from_the_root_hands_the_editor_the_focus(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("cmd-p");

    fail_trash(gate, trashing, &mut cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(editor_has_focus(&window, &mut cx));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_picker_opened_from_the_tree_hands_it_back_the_focus(cx: &mut TestAppContext) {
    let Pending {
        window,
        mut cx,
        gate,
        trashing,
        ..
    } = pending_trash(cx, true);
    cx.simulate_keystrokes("ctrl-tab");
    focus_tree(&window, &mut cx);
    cx.simulate_keystrokes("cmd-p");

    fail_trash(gate, trashing, &mut cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(tree_has_focus(&window, &mut cx));
    check(&window, &mut cx);
}

/// Its view was dropped when the tab arrived on the trashing note, so it parks none (I1 with no view).
#[gpui::test]
fn leaving_a_trashing_tab_and_failing_opens_it_at_the_top(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "line\n".repeat(200));
    fs.insert_file("/vault/b.md", "beta");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, 610..610);
    assert_ne!(scroll_to(&window, &mut cx, 600), at(0));
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    cx.simulate_keystrokes("ctrl-tab");
    fs.make_untrashable("/vault/a.md");
    let trashing = start_trash(&window, &mut cx, a);
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("ctrl-tab");
    check(&window, &mut cx);

    fail_trash(gate, trashing, &mut cx);
    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(view_of(&window, &mut cx), (at(0)..at(0), at(0)));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_trashing_tabs_title_is_its_notes_name(cx: &mut TestAppContext) {
    let Pending { window, mut cx, .. } = pending_trash(cx, true);

    cx.simulate_keystrokes("ctrl-tab");

    let titles = window
        .update(&mut cx, |shell, _, cx| {
            shell.tab_titles(shell.layout.last_center(), cx)
        })
        .unwrap();
    assert_eq!(titles, ["b", "a"]);
    check(&window, &mut cx);
}
