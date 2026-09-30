//! Opening in a new tab: the tab is inserted after the active one when its note lands (tabs S3).

use std::sync::Arc;

use gpui::{
    Focusable, Modifiers, TestAppContext, VisualTestContext, WindowHandle, point, px, size,
};
use inkra_text::ByteOffset;
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::{at, check, editor, open, select, shell_focused, showing, tab_notes, text_of, trash};
use crate::picker::runs::SEARCH_DEBOUNCE;
use crate::shell::AppShell;
use crate::sidebar::row_center;
use crate::test_support;

struct Abc {
    fs: Arc<MemFs>,
    window: WindowHandle<AppShell>,
    cx: VisualTestContext,
    a: EntryId,
    b: EntryId,
    c: EntryId,
}

/// `a`, `b`, `c` in sidebar rows 0, 1, 2.
fn abc(cx: &mut TestAppContext) -> Abc {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.insert_file("/vault/c.md", "gamma");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    let c = test_support::entry_id(&notes, &mut cx, "c");
    Abc {
        fs,
        window,
        cx,
        a,
        b,
        c,
    }
}

fn cmd_click_row(cx: &mut VisualTestContext, row: f32) {
    cx.simulate_click(point(px(20.), row_center(row)), Modifiers::command());
    cx.run_until_parked();
}

fn caret(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> ByteOffset {
    editor(window, cx).update(cx, |editor, _| editor.cursor())
}

#[gpui::test]
fn cmd_click_on_a_sidebar_row_opens_it_in_a_new_tab_after_the_active_one(cx: &mut TestAppContext) {
    let Abc {
        window,
        mut cx,
        a,
        b,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    cx.simulate_keystrokes("cmd-1");

    cmd_click_row(&mut cx, 1.);

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), None], 1)
    );
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_new_tab_opened_from_an_empty_tab_keeps_the_empty_tab(cx: &mut TestAppContext) {
    let Abc {
        window,
        mut cx,
        a,
        b,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");

    cmd_click_row(&mut cx, 1.);

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), None, Some(b)], 2)
    );
    check(&window, &mut cx);
}

/// Caret 5 on return, not 3: the tab left was parked as anchors, not captured as offsets.
#[gpui::test]
fn a_new_tab_on_the_note_showing_duplicates_its_view_and_parks_the_tab_left(
    cx: &mut TestAppContext,
) {
    let Abc {
        window, mut cx, a, ..
    } = abc(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);

    cmd_click_row(&mut cx, 0.);

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(a)], 1));
    assert_eq!(caret(&window, &mut cx), at(3));
    check(&window, &mut cx);

    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input("xy");
    cx.simulate_keystrokes("ctrl-tab");

    assert_eq!(caret(&window, &mut cx), at(5));
    check(&window, &mut cx);
}

#[gpui::test]
fn the_new_tab_takes_focus_and_keys(cx: &mut TestAppContext) {
    let Abc {
        window,
        mut cx,
        a,
        b,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);

    cmd_click_row(&mut cx, 1.);

    let focused = window
        .update(&mut cx, |shell, window, cx| {
            shell
                .editor()
                .is_some_and(|editor| editor.focus_handle(cx).is_focused(window))
        })
        .unwrap();
    assert!(focused, "the editor of the new tab has the focus");
    cx.simulate_input("z");
    assert_eq!(text_of(&window, &mut cx, b), "zbeta");
    check(&window, &mut cx);
}

#[gpui::test]
fn the_later_of_two_pending_new_tab_opens_wins(cx: &mut TestAppContext) {
    let Abc {
        fs,
        window,
        mut cx,
        a,
        c,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/b.md");

    cmd_click_row(&mut cx, 1.);
    cmd_click_row(&mut cx, 2.);
    gate.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(c)], 1));
    check(&window, &mut cx);
}

/// T4: a `⌘`-click then `ctrl-tab` before the load lands opens nothing.
#[gpui::test]
fn a_tab_switch_cancels_a_pending_new_tab_open(cx: &mut TestAppContext) {
    let Abc {
        fs,
        window,
        mut cx,
        a,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    let gate = fs.block_load("/vault/b.md");

    cmd_click_row(&mut cx, 1.);
    cx.simulate_keystrokes("ctrl-tab");
    gate.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), None], 0));
    check(&window, &mut cx);
}

/// As `a_tab_activated_past_the_bar_edge_scrolls_into_view`: 31 tabs cannot fit 700 px.
#[gpui::test]
fn a_new_tab_opened_past_the_bar_edge_scrolls_into_view(cx: &mut TestAppContext) {
    let Abc {
        window, mut cx, a, ..
    } = abc(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    cx.simulate_keystrokes("cmd-9");
    cx.run_until_parked();

    cmd_click_row(&mut cx, 1.);

    assert_eq!(tab_notes(&window, &mut cx).1, 30);
    let bar = cx.debug_bounds("tab-bar").expect("the bar is drawn");
    let new = cx.debug_bounds("tab-30").expect("the new tab is drawn");
    assert!(
        new.left() >= bar.left() && new.right() <= bar.right(),
        "tab-30 at {new:?} is outside the bar at {bar:?}"
    );
}

#[gpui::test]
fn a_note_closed_on_the_active_tab_cancels_a_pending_new_tab_open(cx: &mut TestAppContext) {
    let Abc {
        fs,
        window,
        mut cx,
        a,
        ..
    } = abc(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/b.md");

    cmd_click_row(&mut cx, 1.);
    trash(&window, &mut cx, a);
    gate.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![None], 0));
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
}

#[gpui::test]
fn cmd_click_on_a_backlink_opens_its_source_in_a_new_tab(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha links [[b]]");
    fs.insert_file("/vault/b.md", "beta");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    open(&window, &mut cx, b);

    let row = cx
        .debug_bounds("backlink-0")
        .expect("the panel shows a's backlink");
    cx.simulate_click(row.center(), Modifiers::command());
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(a)], 1));
    check(&window, &mut cx);
}

fn has_modal(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.modal.is_some())
        .unwrap()
}

#[gpui::test]
fn cmd_enter_in_quick_open_opens_the_pick_in_a_new_tab(cx: &mut TestAppContext) {
    let Abc {
        window,
        mut cx,
        a,
        b,
        ..
    } = abc(cx);
    open(&window, &mut cx, b);
    open(&window, &mut cx, a);

    // The recent notes, the one showing left out: `b` first.
    cx.simulate_keystrokes("cmd-p");
    cx.simulate_keystrokes("cmd-enter");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    assert!(!has_modal(&window, &mut cx));
    cx.simulate_input("z");
    assert_eq!(text_of(&window, &mut cx, b), "zbeta");
    check(&window, &mut cx);
}

#[gpui::test]
fn cmd_enter_in_vault_search_opens_the_hit_in_a_new_tab_with_it_selected(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha plan");
    fs.insert_file("/vault/b.md", "beta\nplan");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    open(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-shift-f p l a n");
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
    // a's header, a's hit, b's header, b's hit.
    cx.simulate_keystrokes("down cmd-enter");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    let selected =
        editor(&window, &mut cx).update(&mut cx, |editor, _| editor.view_state().selection.range());
    assert_eq!(selected, at(5)..at(9));
    check(&window, &mut cx);
}

/// A command has no tab to open in: `⌘↵` runs it, as `enter`.
#[gpui::test]
fn cmd_enter_in_the_palette_runs_the_command(cx: &mut TestAppContext) {
    let Abc {
        window, mut cx, a, ..
    } = abc(cx);
    open(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-shift-p n e w space t a b cmd-enter");
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), None], 1));
    check(&window, &mut cx);
}
