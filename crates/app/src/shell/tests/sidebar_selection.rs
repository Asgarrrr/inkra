use gpui::TestAppContext;

use super::{click_row, focus_tree, four_notes, keys, open, selected, showing};

#[gpui::test]
fn the_sidebar_selects_the_active_tabs_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);

    keys(&mut cx, "ctrl-tab");
    assert_eq!(selected(&window, &mut cx), Some(a));
    keys(&mut cx, "ctrl-tab");
    assert_eq!(selected(&window, &mut cx), Some(b));
}

#[gpui::test]
fn back_moves_the_sidebar_selection(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    assert_eq!(selected(&window, &mut cx), Some(b));

    keys(&mut cx, "cmd-[");

    assert_eq!(selected(&window, &mut cx), Some(a));
}

#[gpui::test]
fn a_note_opened_from_quick_open_is_selected_in_the_sidebar(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);

    keys(&mut cx, "cmd-p");
    keys(&mut cx, "enter");

    assert_eq!(showing(&window, &mut cx), Some(a));
    assert_eq!(selected(&window, &mut cx), Some(a));
}

#[gpui::test]
fn an_empty_tab_keeps_the_sidebar_selection(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, ..]) = four_notes(cx);
    open(&window, &mut cx, a);

    keys(&mut cx, "cmd-t");

    assert_eq!(selected(&window, &mut cx), Some(a));
}

/// The tree moves `selected` to the neighbour before the delete; the `NoteClosed` after must keep it.
#[gpui::test]
fn a_trash_from_the_tree_keeps_its_neighbour_selected(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [_, b, c, _]) = four_notes(cx);
    open(&window, &mut cx, b);
    focus_tree(&window, &mut cx);

    keys(&mut cx, "cmd-backspace");
    cx.simulate_prompt_answer("Move to Trash");
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), None);
    assert_eq!(selected(&window, &mut cx), Some(c));
}

#[gpui::test]
fn a_tab_switch_while_renaming_keeps_the_selection(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-1");
    focus_tree(&window, &mut cx);
    keys(&mut cx, "enter");
    let renaming = window
        .update(&mut cx, |shell, _, cx| {
            shell.panes().sidebar.read(cx).is_editing()
        })
        .unwrap();
    assert!(renaming);

    keys(&mut cx, "cmd-2");

    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(
        selected(&window, &mut cx),
        Some(a),
        "the field keeps its row"
    );
}

/// A tab switch is synchronous: it moves the cursor to the note it shows, over a pending click.
#[gpui::test]
fn a_superseded_activation_leaves_the_sidebar_selection_on_the_note_showing(
    cx: &mut TestAppContext,
) {
    let (fs, window, mut cx, [a, b, ..]) = four_notes(cx);
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    let gate = fs.block_load("/vault/a.md");
    click_row(&mut cx, 0.);
    assert_eq!(selected(&window, &mut cx), Some(a));

    keys(&mut cx, "ctrl-tab");
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(b));
    assert_eq!(selected(&window, &mut cx), Some(b));
}

#[gpui::test]
fn a_click_on_a_note_row_selects_it_at_once(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [_, b, ..]) = four_notes(cx);
    let gate = fs.block_load("/vault/b.md");

    click_row(&mut cx, 1.);

    assert_eq!(selected(&window, &mut cx), Some(b));
    gate.release();
}

/// The landing must not move a cursor the user moved after the click: a tree command acts on it.
#[gpui::test]
fn a_landing_keeps_a_tree_cursor_moved_since_its_open_began(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, _, c, d]) = four_notes(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/c.md");
    click_row(&mut cx, 2.);
    keys(&mut cx, "down");
    assert_eq!(selected(&window, &mut cx), Some(d));

    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, &mut cx), Some(c));
    assert_eq!(selected(&window, &mut cx), Some(d));
}
