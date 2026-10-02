use std::sync::Arc;

use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;

use super::{activate, editor, row_point, showing, tree_focused, two_notes};
use crate::open::Open;
use crate::picker::Picker;
use crate::picker::letters::{DELAY, Letters};
use crate::shell::{AppShell, SIDEBAR_WIDTH};

const ROWS: &[&str] = &["apple", "apricot", "beet"];

fn open_letters<const KIND: u8>(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Option<Entity<Picker<Letters<KIND>>>> {
    let picker = window
        .update(cx, |shell, window, cx| {
            shell.toggle_picker(window, cx, |_| Letters::<KIND>::new(ROWS))
        })
        .expect("the shell is open");
    cx.run_until_parked();
    cx.executor().advance_clock(DELAY);
    cx.run_until_parked();
    picker
}

fn has_modal(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.modal.is_some())
        .expect("the shell is open")
}

fn is_focused(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    handle: impl Fn(&AppShell, &gpui::App) -> gpui::FocusHandle,
) -> bool {
    window
        .update(cx, |shell, window, cx| handle(shell, cx).is_focused(window))
        .expect("the shell is open")
}

#[gpui::test]
fn a_picker_opens_over_the_window_and_takes_focus(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);

    let picker = open_letters::<0>(window, &mut visual).expect("it opened");

    assert!(has_modal(window, &mut visual));
    let focused = window
        .update(&mut visual, |_, window, cx| {
            picker.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(focused, "the picker's field has focus");
}

#[gpui::test]
fn escape_gives_focus_back_to_the_tree(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);
    open_letters::<0>(window, &mut visual);

    visual.simulate_keystrokes("escape");

    assert!(!has_modal(window, &mut visual));
    assert!(is_focused(window, &mut visual, AppShell::tree_focus_handle));
}

#[gpui::test]
fn with_nothing_focused_before_escape_focuses_the_shell(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, _sidebar, _notes, _a, _b) = two_notes(cx, &fs);
    cx.update(|cx| cx.bind_keys(crate::keymap::key_bindings()));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    open_letters::<0>(window, &mut visual);

    visual.simulate_keystrokes("escape");

    assert!(is_focused(window, &mut visual, AppShell::focus_handle));
}

#[gpui::test]
fn opening_the_same_kind_again_closes_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);
    open_letters::<0>(window, &mut visual);

    let second = open_letters::<0>(window, &mut visual);

    assert!(second.is_none());
    assert!(!has_modal(window, &mut visual));
    assert!(is_focused(window, &mut visual, AppShell::tree_focus_handle));
}

#[gpui::test]
fn another_kind_replaces_it_and_keeps_the_first_focus(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);
    open_letters::<0>(window, &mut visual);

    let other = open_letters::<1>(window, &mut visual);
    assert!(other.is_some());
    assert!(has_modal(window, &mut visual));

    visual.simulate_keystrokes("escape");
    assert!(!has_modal(window, &mut visual));
    assert!(
        is_focused(window, &mut visual, AppShell::tree_focus_handle),
        "not the first picker's field"
    );
}

#[gpui::test]
fn a_click_on_the_backdrop_over_a_sidebar_row_dismisses_without_activating_it(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);
    open_letters::<0>(window, &mut visual);

    let at = row_point(&mut visual, 1.);
    visual.simulate_click(at, Modifiers::none());

    assert!(!has_modal(window, &mut visual));
    assert_eq!(showing(&window, cx), Some(a), "b's row never saw the click");
}

#[gpui::test]
fn a_click_on_a_picker_row_does_not_reach_the_editor(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    // Replaces `two_notes`' text so the editor has a line under every picker row.
    fs.insert_file("/vault/a.md", "alpha\n".repeat(40));
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    let before = editor(&window, cx).update(cx, |editor, _| editor.cursor());
    let picker = open_letters::<0>(window, &mut visual).expect("it opened");
    let row = visual.debug_bounds("picker-row-2").expect("row 2 is drawn");
    assert!(row.left() > SIDEBAR_WIDTH, "the row lies over the editor");

    visual.simulate_click(row.center(), Modifiers::none());

    assert_eq!(
        picker.read_with(&visual, |p, _| p.delegate.confirmed.borrow().clone()),
        [("beet", Open::Here)]
    );
    assert_eq!(
        editor(&window, cx).update(cx, |editor, _| editor.cursor()),
        before
    );
    assert!(!has_modal(window, &mut visual));
}

#[gpui::test]
fn a_click_on_the_query_field_keeps_the_picker_open(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    let mut visual = tree_focused(cx, window, &sidebar);
    let picker = open_letters::<0>(window, &mut visual).expect("it opened");
    let field = visual
        .debug_bounds("picker-field")
        .expect("the field is drawn");

    visual.simulate_click(field.center(), Modifiers::none());

    assert!(has_modal(window, &mut visual));
    let focused = window
        .update(&mut visual, |_, window, cx| {
            picker.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(focused, "the picker's field keeps focus");
}

#[gpui::test]
fn focus_leaving_the_picker_dismisses_it_and_stays_where_it_went(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);
    open_letters::<0>(window, &mut visual);

    // Test activation lands asynchronously, and a blur counts only from a frame drawn active.
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    window
        .update(&mut visual, |shell, window, cx| {
            let editor = shell.focus_handle(cx);
            window.focus(&editor, cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();

    assert!(!has_modal(window, &mut visual));
    assert!(
        is_focused(window, &mut visual, AppShell::focus_handle),
        "not sent back to the tree"
    );
}
