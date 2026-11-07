//! A vault launch puts back the saved tabs once the vault is walked whole, unless the user already holds a working set (restore-tabs spec, R6).

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui::{
    Bounds, Focusable, TestAppContext, VisualTestContext, WindowBounds, WindowHandle, WindowOptions,
};
use inkra_layout::{Axis, Member, Side, View};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Vault};

use super::dock_menu::{default_docks, docks, side_of};
use super::session_writes::{SESSION, saved_tabs, tab, wait};
use super::splits::{centre, close, editor_in, last, shown_in, tabs_in};
use super::{check, tab_files};
use crate::bootstrap::{Launch, root};
use crate::images::png;
use crate::keymap::key_bindings;
use crate::restore::{SavedAxis, SavedFileTab, SavedGroup, SavedMember, SavedTab, Session};
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

/// One centre group of `tabs`, and docks rule 6's docks.
pub(super) fn save(fs: &MemFs, active: usize, tabs: Vec<SavedFileTab>) -> String {
    let center = notes_group(tabs, active);
    let session = Session::new("/vault".into(), 0, center, None, None, [true, true]);
    save_text(fs, session.encode().unwrap())
}

/// One tab on `a.md`, and `left` as the left dock.
fn save_docks(fs: &MemFs, left: SavedMember) {
    let center = notes_group(vec![tab("a.md", 0, 0)], 0);
    let session = Session::new("/vault".into(), 0, center, Some(left), None, [true, true]);
    save_text(fs, session.encode().unwrap());
}

/// `center`, and docks rule 6's docks.
pub(super) fn save_tree(fs: &MemFs, active: usize, center: SavedMember) {
    let session = Session::new("/vault".into(), active, center, None, None, [true, true]);
    save_text(fs, session.encode().unwrap());
}

pub(super) fn notes_group(tabs: Vec<SavedFileTab>, active: usize) -> SavedMember {
    SavedMember::Group(SavedGroup {
        tabs: tabs.into_iter().map(SavedTab::File).collect(),
        active,
    })
}

pub(super) fn row(children: Vec<SavedMember>) -> SavedMember {
    SavedMember::Split {
        split: SavedAxis::Horizontal,
        flexes: vec![1.0; children.len()],
        children,
    }
}

pub(super) fn save_text(fs: &MemFs, text: String) -> String {
    fs.insert_file(SESSION, text.clone());
    text
}

pub(super) fn empty() -> SavedFileTab {
    SavedFileTab {
        file: None,
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
    let launched = Launch::Vault(Vault::new(Path::new("/vault")), Some(SESSION.into()), None);
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

pub(super) fn files(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> (Vec<Option<EntryId>>, usize) {
    tab_files(window, cx)
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a), Some(c)], 1));
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a), Some(b)], 0));
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a), Some(b)], 0));
}

/// Nothing to restore still ends the restore: the next change is written.
#[gpui::test]
fn all_notes_missing_leave_one_empty_tab_and_writes_resume(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("gone.md", 0, 0), empty()]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    assert_eq!(files(&window, &mut cx), (vec![None], 0));

    cx.simulate_keystrokes("cmd-t");
    wait(&mut cx, THROTTLE);

    assert_eq!(saved_tabs(&fs).unwrap(), [empty(), empty()]);
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
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
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
    assert!(
        !editor_focused(&window, &mut cx),
        "the picker keeps the keys"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(editor_focused(&window, &mut cx));
}

fn backlinks_left() -> SavedMember {
    let views = [View::Files, View::Backlinks].map(|view| SavedTab::View {
        view: view.name().to_owned(),
    });
    SavedMember::Group(SavedGroup {
        tabs: views.into(),
        active: 1,
    })
}

#[gpui::test]
fn a_saved_layout_is_restored(cx: &mut TestAppContext) {
    let fs = vault();
    save_docks(&fs, backlinks_left());
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(side_of(&window, &mut cx, View::Backlinks), Side::Left);
    assert!(cx.debug_bounds("dock-bar-left").is_some());
}

/// Nothing run after the launch: the window's first draw is the only one.
#[gpui::test]
fn a_saved_layout_is_drawn_by_the_first_frame(cx: &mut TestAppContext) {
    let fs = vault();
    save_docks(&fs, backlinks_left());
    let (window, mut cx) = launch(cx, &fs);

    assert!(cx.debug_bounds("dock-bar-left").is_some());
    assert_eq!(side_of(&window, &mut cx, View::Backlinks), Side::Left);
}

/// The restore reuses the session `bootstrap::root` read before the first frame.
#[gpui::test]
fn the_session_file_is_read_once(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(fs.loads(Path::new(SESSION)), 1);
}

#[gpui::test]
fn a_file_without_docks_keeps_the_default_layout(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    assert_eq!(docks(&window, &mut cx), default_docks());
}

fn selected_in(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    k: usize,
) -> Range<ByteOffset> {
    editor_in(window, cx, k).update(cx, |editor, _| editor.selected())
}

#[gpui::test]
fn a_split_layout_is_restored(cx: &mut TestAppContext) {
    let fs = vault();
    let center = SavedMember::Split {
        split: SavedAxis::Horizontal,
        flexes: vec![1.5, 0.5],
        children: vec![
            notes_group(vec![tab("a.md", 1, 1), tab("b.md", 0, 2)], 1),
            notes_group(vec![tab("f/c.md", 1, 3)], 0),
        ],
    };
    save_tree(&fs, 1, center);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, b, c) = (
        id(&window, &mut cx, "a"),
        id(&window, &mut cx, "b"),
        id(&window, &mut cx, "c"),
    );
    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(tabs_in(&window, &mut cx, 0), [Some(a), Some(b)]);
    assert_eq!(shown_in(&window, &mut cx, 0), Some(b));
    assert_eq!(selected_in(&window, &mut cx, 0), at(0..2));
    assert_eq!(tabs_in(&window, &mut cx, 1), [Some(c)]);
    assert_eq!(selected_in(&window, &mut cx, 1), at(1..3));
    assert_eq!(last(&window, &mut cx), groups[1]);
    let center = window
        .update(&mut cx, |shell, _, _| shell.layout.center().clone())
        .unwrap();
    let Member::Split { axis, flexes, .. } = center else {
        panic!("a split centre: {center:?}");
    };
    assert_eq!((axis, flexes), (Axis::Horizontal, vec![1.5, 0.5]));
    assert!(editor_focused(&window, &mut cx));
    close(&mut cx);
}

#[gpui::test]
fn a_group_whose_notes_are_gone_is_removed(cx: &mut TestAppContext) {
    let fs = vault();
    let center = row(vec![
        notes_group(vec![tab("gone.md", 0, 0), empty()], 0),
        notes_group(vec![tab("a.md", 2, 2)], 0),
        notes_group(vec![tab("gone.md", 0, 0)], 0),
    ]);
    save_tree(&fs, 0, center);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 1);
    assert_eq!(last(&window, &mut cx), groups[0]);
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(selection(&window, &mut cx), at(2..2));
    assert!(editor_focused(&window, &mut cx));
}

#[gpui::test]
fn the_active_group_maps_to_a_survivor(cx: &mut TestAppContext) {
    let fs = vault();
    let center = row(vec![
        notes_group(vec![tab("a.md", 0, 0)], 0),
        notes_group(vec![tab("b.md", 0, 0)], 0),
        notes_group(vec![tab("gone.md", 0, 0)], 0),
        notes_group(vec![tab("f/c.md", 0, 0)], 0),
    ]);
    save_tree(&fs, 2, center);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, b) = (id(&window, &mut cx, "a"), id(&window, &mut cx, "b"));
    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 3);
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    let current = window
        .update(&mut cx, |shell, _, cx| shell.notes.read(cx).current())
        .unwrap();
    assert_eq!(current, Some(b));
    assert_eq!(shown_in(&window, &mut cx, 0), Some(a));
    close(&mut cx);
}

/// The layout restores before the walk, its active group with it: a load held back leaves the saved one active.
#[gpui::test]
fn the_saved_active_group_is_active_before_the_notes_land(cx: &mut TestAppContext) {
    let fs = vault();
    save_tree(
        &fs,
        1,
        row(vec![
            notes_group(vec![tab("a.md", 0, 0)], 0),
            notes_group(vec![tab("b.md", 0, 0)], 0),
        ]),
    );
    let gate = fs.block_load("/vault/b.md");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let groups = centre(&window, &mut cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(last(&window, &mut cx), groups[1]);
    gate.release();
    cx.run_until_parked();

    let b = id(&window, &mut cx, "b");
    assert_eq!(last(&window, &mut cx), groups[1]);
    assert_eq!(shown_in(&window, &mut cx, 1), Some(b));
    close(&mut cx);
}

#[gpui::test]
fn a_note_saved_in_a_dock_is_dropped(cx: &mut TestAppContext) {
    let fs = vault();
    let left = SavedGroup {
        tabs: vec![
            SavedTab::File(tab("a.md", 0, 0)),
            SavedTab::View {
                view: "files".to_owned(),
            },
        ],
        active: 0,
    };
    let center = notes_group(vec![tab("b.md", 0, 0)], 0);
    let session = Session::new(
        "/vault".into(),
        0,
        center,
        Some(SavedMember::Group(left)),
        None,
        [true, true],
    );
    save_text(&fs, session.encode().unwrap());
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let b = id(&window, &mut cx, "b");
    assert_eq!(docks(&window, &mut cx), default_docks());
    assert_eq!(files(&window, &mut cx), (vec![Some(b)], 0));
}

#[gpui::test]
fn a_view_saved_in_the_centre_is_dropped(cx: &mut TestAppContext) {
    let fs = vault();
    let center = SavedMember::Group(SavedGroup {
        tabs: vec![
            SavedTab::View {
                view: "backlinks".to_owned(),
            },
            SavedTab::File(tab("a.md", 0, 0)),
        ],
        active: 0,
    });
    save_tree(&fs, 0, center);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(docks(&window, &mut cx), default_docks());
}

/// Only the shown image is read at the landing; a parked one waits for its first switch.
#[gpui::test]
fn a_shown_image_tab_is_restored_and_a_parked_one_is_not_read(cx: &mut TestAppContext) {
    let fs = vault();
    fs.insert_bytes("/vault/photo.png", png(40, 30));
    fs.insert_bytes("/vault/other.png", png(30, 40));
    let image = |path: &str| SavedFileTab {
        file: Some(path.to_owned()),
        view: None,
    };
    save(
        &fs,
        0,
        vec![image("photo.png"), tab("a.md", 1, 3), image("other.png")],
    );
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let [photo, a, other] = ["photo.png", "a", "other.png"].map(|name| id(&window, &mut cx, name));
    assert_eq!(
        files(&window, &mut cx),
        (vec![Some(photo), Some(a), Some(other)], 0)
    );
    assert!(cx.debug_bounds("image-view").is_some());
    assert_eq!(fs.reads(Path::new("/vault/photo.png")), 1);
    assert_eq!(fs.reads(Path::new("/vault/other.png")), 0);
    check(&window, &mut cx);

    cx.simulate_keystrokes("cmd-3");
    cx.run_until_parked();

    assert_eq!(fs.reads(Path::new("/vault/other.png")), 1);
    check(&window, &mut cx);
}

/// The landing runs after the vault's last notify, so it closes the tab of an asset deleted since the plan resolved.
#[gpui::test]
fn an_image_deleted_before_the_landing_closes_its_tab(cx: &mut TestAppContext) {
    let fs = vault();
    fs.insert_bytes("/vault/photo.png", png(40, 30));
    let image = SavedFileTab {
        file: Some("photo.png".to_owned()),
        view: None,
    };
    save(&fs, 0, vec![image, tab("a.md", 0, 0)]);
    let gate = fs.block_load("/vault/a.md");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    fs.remove(Path::new("/vault/photo.png"));
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    assert_eq!(files(&window, &mut cx), (vec![Some(a)], 0));
    check(&window, &mut cx);
}
