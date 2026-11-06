use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::GroupId;
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::dragging::drag;
use super::{
    AppShell, EditorView, ab_focused, at, keys, open, select, shell_focused, start_open, trash,
};
use crate::shell::group::SplitRight;
use crate::test_support;

pub(super) fn centre(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<GroupId> {
    window
        .update(cx, |shell, _, _| shell.layout.center_groups())
        .unwrap()
}

pub(super) fn last(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> GroupId {
    window
        .update(cx, |shell, _, _| shell.layout.last_center())
        .unwrap()
}

/// The note bound in centre group `k`.
pub(super) fn shown_in(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    k: usize,
) -> Option<EntryId> {
    let group = centre(window, cx)[k];
    window
        .update(cx, |shell, _, _| {
            shell.group(group).shown.bound().map(|bound| bound.note)
        })
        .unwrap()
}

pub(super) fn editor_in(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    k: usize,
) -> Entity<EditorView> {
    let group = centre(window, cx)[k];
    window
        .update(cx, |shell, _, _| shell.editor_of(group).cloned())
        .unwrap()
        .expect("the group shows a note")
}

fn tab_count(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, k: usize) -> usize {
    let group = centre(window, cx)[k];
    window
        .update(cx, |shell, _, _| shell.group(group).tabs.count())
        .unwrap()
}

pub(super) fn tabs_in(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    k: usize,
) -> Vec<Option<EntryId>> {
    let group = centre(window, cx)[k];
    window
        .update(cx, |shell, _, _| shell.tab_notes(group).collect())
        .unwrap()
}

pub(super) fn current(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Option<EntryId> {
    window
        .update(cx, |shell, _, cx| shell.notes.read(cx).current())
        .unwrap()
}

pub(super) fn check_layout(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, _, cx| shell.check_layout(cx))
        .unwrap();
}

pub(super) fn split(cx: &mut VisualTestContext) {
    cx.dispatch_action(SplitRight);
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).expect("drawn").center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

pub(super) fn focused(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    editor: &Entity<EditorView>,
) -> bool {
    window
        .update(cx, |_, window, cx| {
            gpui::Focusable::focus_handle(editor, cx).is_focused(window)
        })
        .unwrap()
}

/// Releases the shell, which unpins its editors: the test harness drops entities without releasing them.
pub(super) fn close(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}

fn cursor(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> inkra_text::ByteOffset {
    editor.read_with(cx, |editor, _| editor.cursor())
}

#[gpui::test]
fn split_right_shows_the_same_note_with_its_own_caret(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 1..1);

    split(&mut cx);

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    let (left, right) = (
        editor_in(&window, &mut cx, 0),
        editor_in(&window, &mut cx, 1),
    );
    assert_ne!(left.entity_id(), right.entity_id());
    assert_eq!(cursor(&right, &mut cx), at(1));
    assert!(focused(&window, &mut cx, &right));

    select(&window, &mut cx, 3..3);
    assert_eq!(cursor(&left, &mut cx), at(1));
    assert_eq!(cursor(&right, &mut cx), at(3));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn typing_in_the_active_group_moves_the_other_groups_caret(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);
    split(&mut cx);
    select(&window, &mut cx, 0..0);

    cx.simulate_input("zz");

    assert_eq!(super::text_of(&window, &mut cx, a), "zzalpha");
    let left = editor_in(&window, &mut cx, 0);
    assert_eq!(cursor(&left, &mut cx), at(5));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn a_press_in_a_group_activates_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[b]]");
    fs.insert_file("/vault/b.md", "[[a]]");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    open(&window, &mut cx, a);
    split(&mut cx);
    open(&window, &mut cx, b);
    let sources = |window: &WindowHandle<AppShell>, cx: &mut VisualTestContext| {
        window
            .update(cx, |shell, _, cx| {
                shell.panes().backlinks.read(cx).source_names()
            })
            .unwrap()
    };
    assert_eq!(sources(&window, &mut cx), ["a"]);

    press(&mut cx, "note-column");

    let groups = centre(&window, &mut cx);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(current(&window, &mut cx), Some(a));
    assert_eq!(sources(&window, &mut cx), ["b"]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    let left = editor_in(&window, &mut cx, 0);
    assert!(focused(&window, &mut cx, &left));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn an_open_lands_in_the_group_it_began_in(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    let gate = fs.block_load("/vault/b.md");
    start_open(&window, &mut cx, b);

    press(&mut cx, "note-column");
    gate.release();
    cx.run_until_parked();

    let groups = centre(&window, &mut cx);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    assert_eq!(current(&window, &mut cx), Some(a));
    let left = editor_in(&window, &mut cx, 0);
    assert!(focused(&window, &mut cx, &left));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn quick_open_opens_in_the_active_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    press(&mut cx, "note-column");

    keys(&mut cx, "cmd-p");
    keys(&mut cx, "b");
    keys(&mut cx, "enter");

    assert_eq!(shown_in(&window, &mut cx, 0), Some(b));
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    assert_eq!(current(&window, &mut cx), Some(b));
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn cmd_w_closes_in_the_active_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    keys(&mut cx, "cmd-t");
    assert_eq!(tab_count(&window, &mut cx, 1), 2);

    keys(&mut cx, "cmd-w");

    assert_eq!(tab_count(&window, &mut cx, 0), 2);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b)]);

    press(&mut cx, "note-column");
    keys(&mut cx, "cmd-w");

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b)]);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn cmd_s_saves_the_active_groups_note(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    open(&window, &mut cx, b);
    press(&mut cx, "note-column");
    cx.simulate_input("X");
    press(&mut cx, "note-column-1");
    cx.simulate_input("Y");
    let disk = |path| fs.read_to_string(Path::new(path));
    // Leaving group 0 saved its note at once, so only the active group's note is still dirty.
    assert_eq!(disk("/vault/a.md").as_deref(), Some("alphaX"));
    assert_eq!(disk("/vault/b.md").as_deref(), Some("beta"));

    keys(&mut cx, "cmd-s");

    assert_eq!(disk("/vault/b.md").as_deref(), Some("betaY"));
    assert_eq!(disk("/vault/a.md").as_deref(), Some("alphaX"));
    close(&mut cx);
}

#[gpui::test]
fn trashing_a_note_shown_twice_empties_both_groups(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);

    trash(&window, &mut cx, a);

    assert_eq!(shown_in(&window, &mut cx, 0), None);
    assert_eq!(shown_in(&window, &mut cx, 1), None);
    assert_eq!(tabs_in(&window, &mut cx, 0), [None]);
    assert_eq!(current(&window, &mut cx), None);
    check_layout(&window, &mut cx);
    close(&mut cx);
}

#[gpui::test]
fn each_strip_sits_over_its_column(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    split(&mut cx);
    split(&mut cx);
    assert_eq!(centre(&window, &mut cx).len(), 3);

    for (strip, column) in [
        ("tab-bar", "note-column"),
        ("tab-bar-1", "note-column-1"),
        ("tab-bar-2", "note-column-2"),
    ] {
        let strip = cx.debug_bounds(strip).expect("drawn").left();
        let column = cx.debug_bounds(column).expect("drawn").left();
        assert!(
            (strip - column).abs() < gpui::px(0.01),
            "{strip:?} vs {column:?}"
        );
    }
    close(&mut cx);
}

#[gpui::test]
fn the_palette_lists_split_right(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);

    keys(&mut cx, "cmd-shift-p");
    let names = window
        .update(&mut cx, |shell, _, cx| shell.palette_names(cx))
        .unwrap()
        .expect("the palette is open");
    assert!(
        names.iter().any(|label| label == "Split right, ⌘K →"),
        "{names:?}"
    );

    keys(&mut cx, "s p l i t space r i g h t enter");
    assert_eq!(centre(&window, &mut cx).len(), 2);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(a));
    close(&mut cx);
}

/// The root holds focus, so landing it on the note would move it.
#[gpui::test]
fn a_split_refused_by_the_minimum_leaves_the_focus_where_it_was(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(gpui::size(gpui::px(650.), gpui::px(600.)));
    open(&window, &mut cx, a);
    let root = window
        .update(&mut cx, |shell, window, cx| {
            window.focus(&shell.focus_handle, cx);
            shell.focus_handle.clone()
        })
        .unwrap();
    cx.run_until_parked();

    keys(&mut cx, "cmd-k right");

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert!(
        window
            .update(&mut cx, |_, window, _| root.is_focused(window))
            .unwrap()
    );
    close(&mut cx);
}

#[gpui::test]
fn a_note_moved_to_another_group_becomes_active_there(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    split(&mut cx);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, a);
    let groups = centre(&window, &mut cx);
    assert_eq!(last(&window, &mut cx), groups[1]);

    drag(&mut cx, "tab-1-1", "tab-0");

    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(a), Some(b)]);
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(b)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    assert_eq!(last(&window, &mut cx), groups[0]);
    let editor = editor_in(&window, &mut cx, 0);
    assert!(focused(&window, &mut cx, &editor));
    check_layout(&window, &mut cx);
    close(&mut cx);
}
