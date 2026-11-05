//! A vault launch puts back the saved tabs once the vault is walked whole, unless the user already holds a working set (restore-tabs spec, R6).

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui::{
    Bounds, Focusable, TestAppContext, VisualTestContext, WindowBounds, WindowHandle, WindowOptions,
};
use inkra_layout::{Docks, Side, View};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use super::session_writes::{SESSION, saved, tab, wait};
use super::{check, tab_notes};
use crate::bootstrap::{Launch, root};
use crate::keymap::key_bindings;
use crate::restore::{SavedDocks, SavedTab, Session};
use crate::shell::AppShell;
use crate::shell::session::THROTTLE;
use crate::test_support;

/// `a.md` ("alpha"), `b.md` ("beta") and `f/c.md` ("gamma").
pub(super) fn vault() -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    fs.insert_file("/vault/f/c.md", "gamma");
    fs
}

pub(super) fn save(fs: &MemFs, active: usize, tabs: Vec<SavedTab>) -> String {
    save_session(fs, &Session::new("/vault".into(), active, tabs, None))
}

/// One tab on `a.md`, laid out as `docks`.
fn save_docks(fs: &MemFs, docks: &Docks) {
    let tabs = vec![tab("a.md", 0, 0)];
    let docks = Some(SavedDocks::from(docks));
    save_session(fs, &Session::new("/vault".into(), 0, tabs, docks));
}

fn save_session(fs: &MemFs, session: &Session) -> String {
    let text = session.encode().unwrap();
    fs.insert_file(SESSION, text.clone());
    text
}

pub(super) fn empty() -> SavedTab {
    SavedTab {
        note: None,
        view: None,
    }
}

/// A vault launch on `fs`, unscanned, with `SESSION` as its session file; the root focused, nothing run yet.
pub(super) fn launch(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::clone(fs) as Arc<dyn Fs>;
    let launched = Launch::Vault(Vault::new(Path::new("/vault")), Some(SESSION.into()));
    let window = cx
        .update(|cx| {
            let bounds = Bounds::maximized(None, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| root(fs, launched, window, cx),
            )
        })
        .unwrap();
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

pub(super) fn id(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    name: &str,
) -> EntryId {
    let notes = window
        .update(cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    test_support::entry_id(&notes, cx, name)
}

pub(super) fn notes(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> (Vec<Option<EntryId>>, usize) {
    tab_notes(window, cx)
}

pub(super) fn selection(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Range<ByteOffset> {
    super::editor(window, cx).update(cx, |editor, _| editor.selected())
}

pub(super) fn editor_focused(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell
                .editor()
                .is_some_and(|editor| editor.focus_handle(cx).is_focused(window))
        })
        .unwrap()
}

pub(super) fn at(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}

#[gpui::test]
fn the_saved_tabs_come_back_in_order_with_the_active_one_and_its_view(cx: &mut TestAppContext) {
    let fs = vault();
    save(
        &fs,
        1,
        vec![tab("a.md", 1, 3), tab("f/c.md", 4, 2), empty()],
    );
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, c) = (id(&window, &mut cx, "a"), id(&window, &mut cx, "c"));
    assert_eq!(notes(&window, &mut cx), (vec![Some(a), Some(c)], 1));
    assert_eq!(selection(&window, &mut cx), at(2..4));
    assert!(
        editor_focused(&window, &mut cx),
        "the note takes the root's focus"
    );
    check(&window, &mut cx);
}

/// Parked as anchors in the buffer the landing loaded.
#[gpui::test]
fn a_restored_inactive_tab_shows_its_saved_view_when_activated(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 1, vec![tab("a.md", 1, 3), tab("b.md", 2, 2)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();

    assert_eq!(selection(&window, &mut cx), at(1..3));
    check(&window, &mut cx);
}

/// Anchored like any parked view: typing in another tab on the same note moves it.
#[gpui::test]
fn a_restored_inactive_view_follows_an_edit_made_in_another_tab(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 1, vec![tab("a.md", 3, 3), tab("a.md", 0, 0)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_input("xy");
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();

    assert_eq!(selection(&window, &mut cx), at(5..5));
    check(&window, &mut cx);
}

/// Listed by the walk, refused at its load: dropped as a missing note, and the active tab chosen after.
#[gpui::test]
fn a_note_that_fails_to_load_is_dropped(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 1, vec![tab("a.md", 0, 0), tab("b.md", 0, 0)]);
    let walk = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    fs.make_unreadable("/vault/b.md");
    walk.release();
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_missing_note_is_dropped_and_the_active_tab_moves_left(cx: &mut TestAppContext) {
    let fs = vault();
    save(
        &fs,
        1,
        vec![tab("a.md", 0, 0), tab("gone.md", 0, 0), tab("b.md", 0, 0)],
    );
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, b) = (id(&window, &mut cx, "a"), id(&window, &mut cx, "b"));
    assert_eq!(notes(&window, &mut cx), (vec![Some(a), Some(b)], 0));
}

#[gpui::test]
fn a_missing_first_active_tab_gives_way_to_the_first_survivor(cx: &mut TestAppContext) {
    let fs = vault();
    save(
        &fs,
        0,
        vec![tab("gone.md", 0, 0), tab("a.md", 0, 0), tab("b.md", 0, 0)],
    );
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, b) = (id(&window, &mut cx, "a"), id(&window, &mut cx, "b"));
    assert_eq!(notes(&window, &mut cx), (vec![Some(a), Some(b)], 0));
}

/// Nothing to restore still ends the restore: the next change is written.
#[gpui::test]
fn all_notes_missing_leave_one_empty_tab_and_writes_resume(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("gone.md", 0, 0), empty()]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    assert_eq!(notes(&window, &mut cx), (vec![None], 0));

    cx.simulate_keystrokes("cmd-t");
    wait(&mut cx, THROTTLE);

    assert_eq!(saved(&fs).unwrap().tabs, [empty(), empty()]);
}

/// Opened and dismissed, the picker leaves nothing to replace; focus returns to the root, then to the note.
#[gpui::test]
fn a_picker_opened_and_dismissed_before_landing_still_restores(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn a_name_being_typed_keeps_its_focus(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    let naming = window
        .update(&mut cx, |shell, window, cx| {
            shell.tree_focus_handle(cx).contains_focused(window, cx)
        })
        .unwrap();
    assert!(naming, "the name field keeps the keys");
    assert!(!editor_focused(&window, &mut cx));
}

#[gpui::test]
fn the_empty_line_is_hidden_until_the_restore_ends(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("gone.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    let lines = |window: &WindowHandle<AppShell>, cx: &mut VisualTestContext| {
        window
            .update(cx, |shell, _, _| shell.empty_lines().to_vec())
            .unwrap()
    };
    assert_eq!(lines(&window, &mut cx), Vec::<&str>::new());

    gate.release();
    cx.run_until_parked();

    assert_eq!(lines(&window, &mut cx), ["Select a note."]);
}

/// Each tab parks its own anchors, even on one note.
#[gpui::test]
fn two_inactive_tabs_on_one_note_keep_their_own_views(cx: &mut TestAppContext) {
    let fs = vault();
    save(
        &fs,
        2,
        vec![tab("a.md", 1, 1), tab("a.md", 3, 3), tab("b.md", 0, 0)],
    );
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    assert_eq!(selection(&window, &mut cx), at(1..1));
    cx.simulate_keystrokes("cmd-2");
    cx.run_until_parked();
    assert_eq!(selection(&window, &mut cx), at(3..3));
    check(&window, &mut cx);
}

/// The picker keeps the keys; its dismissal hands them to the note, not back to the root.
#[gpui::test]
fn a_restore_landing_under_a_picker_focuses_the_note_on_its_dismissal(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();
    let a = id(&window, &mut cx, "a");
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    assert!(
        !editor_focused(&window, &mut cx),
        "the picker keeps the keys"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(editor_focused(&window, &mut cx));
}

fn docks(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Docks {
    window
        .update(cx, |shell, _, _| shell.panes().docks.clone())
        .unwrap()
}

fn backlinks_left() -> Docks {
    let mut docks = Docks::default();
    docks.move_view(View::Backlinks, Side::Left, 1);
    docks
}

#[gpui::test]
fn a_saved_layout_is_restored(cx: &mut TestAppContext) {
    let fs = vault();
    save_docks(&fs, &backlinks_left());
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(docks(&window, &mut cx).side_of(View::Backlinks), Side::Left);
    assert!(cx.debug_bounds("dock-bar-left").is_some());
}

/// Nothing run after the launch: the window's first draw is the only one.
#[gpui::test]
fn a_saved_layout_is_drawn_by_the_first_frame(cx: &mut TestAppContext) {
    let fs = vault();
    save_docks(&fs, &backlinks_left());
    let (window, mut cx) = launch(cx, &fs);

    assert!(cx.debug_bounds("dock-bar-left").is_some());
    assert_eq!(docks(&window, &mut cx).side_of(View::Backlinks), Side::Left);
}

/// The restore reuses the session `bootstrap::root` read before the first frame.
#[gpui::test]
fn the_session_file_is_read_once(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(fs.loads(Path::new(SESSION)), 1);
}

#[gpui::test]
fn a_file_without_docks_keeps_the_default_layout(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(docks(&window, &mut cx), Docks::default());
}
