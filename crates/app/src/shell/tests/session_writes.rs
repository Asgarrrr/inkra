//! The session written behind the user: the snapshot, the throttle and the quit hook (restore-tabs spec, R5).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_layout::{Docks, Side};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use super::{keys, open, select, start_trash, text_of, trash};
use crate::keymap::key_bindings;
use crate::restore::{Opened, SavedDocks, SavedTab, SavedView, Session};
use crate::shell::AppShell;
use crate::shell::session::THROTTLE;
use crate::test_support;

pub(super) const SESSION: &str = "/support/sessions/vault.json";

/// `a.md` ("alpha") and `b.md` ("beta"), a shell writing its session to `SESSION`, its root focused.
pub(super) fn saving(
    cx: &mut TestAppContext,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    EntryId,
    EntryId,
) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let notes = test_support::scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let window = cx.add_window({
        let notes = notes.clone();
        move |window, cx| {
            let opened = Opened {
                file: PathBuf::from(SESSION),
                session: None,
            };
            AppShell::new(None, notes, Some(opened), window, cx)
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(&mut visual, |shell, window, cx| {
            window.focus(&shell.focus_handle, cx);
        })
        .unwrap();
    visual.run_until_parked();
    let a = test_support::entry_id(&notes, &mut visual, "a");
    let b = test_support::entry_id(&notes, &mut visual, "b");
    (fs, window, visual, a, b)
}

pub(super) fn saved(fs: &MemFs) -> Option<Session> {
    let text = fs.read_to_string(Path::new(SESSION))?;
    Some(Session::decode(&text, Path::new("/vault")).expect("a session of this vault"))
}

pub(super) fn tab(note: &str, tail: usize, head: usize) -> SavedTab {
    SavedTab {
        note: Some(note.to_owned()),
        view: Some(SavedView {
            tail,
            head,
            top: 0,
            below: 0.,
        }),
    }
}

pub(super) fn wait(cx: &mut VisualTestContext, time: Duration) {
    cx.executor().advance_clock(time);
    cx.run_until_parked();
}

#[gpui::test]
fn the_session_holds_the_tabs_in_bar_order_the_active_one_and_each_view(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = saving(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 2..4);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    cx.simulate_keystrokes("cmd-2");
    select(&window, &mut cx, 1..3);

    wait(&mut cx, THROTTLE);

    let empty = SavedTab {
        note: None,
        view: None,
    };
    assert_eq!(
        saved(&fs),
        Some(Session::new(
            PathBuf::from("/vault"),
            1,
            vec![tab("a.md", 2, 4), tab("b.md", 1, 3), empty],
            Some(SavedDocks::from(&Docks::default())),
        ))
    );
}

#[gpui::test]
fn a_dock_toggle_is_saved(cx: &mut TestAppContext) {
    let (fs, _window, mut cx, _a, _b) = saving(cx);
    keys(&mut cx, "cmd-|");

    wait(&mut cx, THROTTLE);

    let docks = saved(&fs).and_then(|session| session.docks);
    let docks = docks.expect("a saved layout").docks();
    assert!(!docks.dock(Side::Right).is_shown());
}

/// A parked view is saved where its anchors are now, not where the tab left it.
#[gpui::test]
fn a_parked_view_is_saved_after_an_edit_made_in_another_tab(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = saving(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-up");
    cx.simulate_input("xy");

    wait(&mut cx, THROTTLE);

    let tabs = saved(&fs).expect("a session").tabs;
    assert_eq!(tabs, [tab("a.md", 5, 5), tab("a.md", 2, 2)]);
}

/// A throttle, not a debounce: a change inside the window neither writes early nor pushes the write back.
#[gpui::test]
fn the_throttle_writes_once_per_window(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = saving(cx);
    let half = THROTTLE / 2;
    open(&window, &mut cx, a);
    select(&window, &mut cx, 1..1);
    wait(&mut cx, half);
    select(&window, &mut cx, 2..2);
    assert_eq!(saved(&fs), None, "nothing is written inside the window");

    wait(&mut cx, half);
    let first = saved(&fs).expect("the window's write").tabs;
    assert_eq!(first, [tab("a.md", 2, 2)]);

    select(&window, &mut cx, 3..3);
    wait(&mut cx, half);
    assert_eq!(saved(&fs).expect("a session").tabs, first);
    wait(&mut cx, half);
    assert_eq!(saved(&fs).expect("a session").tabs, [tab("a.md", 3, 3)]);
}

#[gpui::test]
fn quitting_writes_the_session_the_throttle_has_not(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = saving(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 1..3);

    cx.quit();

    assert_eq!(saved(&fs).expect("a session").tabs, [tab("a.md", 1, 3)]);
}

/// A save is no gesture: typing across a write still undoes in one step.
#[gpui::test]
fn a_write_does_not_split_the_undo_group(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = saving(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 5..5);
    cx.simulate_input("x");
    wait(&mut cx, THROTTLE);
    assert!(saved(&fs).is_some(), "the write happened between the keys");
    cx.simulate_input("y");

    cx.simulate_keystrokes("cmd-z");

    assert_eq!(text_of(&window, &mut cx, a), "alpha");
}

/// `a` then `b`, `b` active, both saved: a change that shows nothing new must still arm the throttle.
fn two_tabs_saved(
    cx: &mut TestAppContext,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    EntryId,
) {
    let (fs, window, mut cx, a, b) = saving(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    wait(&mut cx, THROTTLE);
    assert_eq!(saved(&fs).expect("a session").tabs.len(), 2);
    (fs, window, cx, b)
}

fn notes_saved(fs: &MemFs) -> Vec<Option<String>> {
    let tabs = saved(fs).expect("a session").tabs;
    tabs.into_iter().map(|tab| tab.note).collect()
}

#[gpui::test]
fn a_tab_dropped_elsewhere_in_the_bar_is_saved(cx: &mut TestAppContext) {
    let (fs, window, mut cx, _b) = two_tabs_saved(cx);
    window
        .update(&mut cx, |shell, _, cx| {
            let id = shell.tabs.id_at(1).unwrap();
            shell.move_tab(id, 0, cx);
        })
        .unwrap();

    wait(&mut cx, THROTTLE);

    assert_eq!(
        notes_saved(&fs),
        [Some("b.md".to_owned()), Some("a.md".to_owned())]
    );
}

#[gpui::test]
fn an_inactive_tab_closed_is_saved(cx: &mut TestAppContext) {
    let (fs, window, mut cx, _b) = two_tabs_saved(cx);
    window
        .update(&mut cx, |shell, window, cx| {
            let id = shell.tabs.id_at(0).unwrap();
            shell.close_tab(id, window, cx);
        })
        .unwrap();

    wait(&mut cx, THROTTLE);

    assert_eq!(notes_saved(&fs), [Some("b.md".to_owned())]);
}

#[gpui::test]
fn an_inactive_tab_emptied_by_a_trash_is_saved(cx: &mut TestAppContext) {
    let (fs, window, mut cx, b) = two_tabs_saved(cx);
    cx.simulate_keystrokes("ctrl-tab");
    wait(&mut cx, THROTTLE);

    trash(&window, &mut cx, b);
    wait(&mut cx, THROTTLE);

    assert_eq!(notes_saved(&fs), [Some("a.md".to_owned()), None]);
}

/// A rename moves no tab and shows nothing new: only the vault's notify can arm the throttle.
#[gpui::test]
fn a_rename_of_a_note_a_tab_shows_is_saved(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = saving(cx);
    open(&window, &mut cx, a);
    wait(&mut cx, THROTTLE);
    assert_eq!(notes_saved(&fs), [Some("a.md".to_owned())]);

    let notes = window
        .update(&mut cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    let renamed = notes.update(&mut cx, |notes, cx| notes.rename(a, "plan", cx));
    cx.run_until_parked();
    assert!(futures::executor::block_on(renamed), "the rename succeeded");
    wait(&mut cx, THROTTLE);

    assert_eq!(notes_saved(&fs), [Some("plan.md".to_owned())]);
}

#[gpui::test]
fn back_and_forward_are_saved(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = saving(cx);
    open(&window, &mut cx, a);
    open(&window, &mut cx, b);
    wait(&mut cx, THROTTLE);
    assert_eq!(notes_saved(&fs), [Some("b.md".to_owned())]);

    cx.simulate_keystrokes("cmd-[");
    wait(&mut cx, THROTTLE);
    assert_eq!(notes_saved(&fs), [Some("a.md".to_owned())]);

    cx.simulate_keystrokes("cmd-]");
    wait(&mut cx, THROTTLE);
    assert_eq!(notes_saved(&fs), [Some("b.md".to_owned())]);
}

/// R5: a tab whose trash is pending saves empty, not the note on its way out.
#[gpui::test]
fn a_tab_whose_trash_is_pending_is_saved_empty(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = saving(cx);
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    let _gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    cx.simulate_keystrokes("ctrl-tab");
    let _trashing = start_trash(&window, &mut cx, a);
    cx.simulate_keystrokes("ctrl-tab");

    wait(&mut cx, THROTTLE);

    let notes: Vec<_> = saved(&fs)
        .expect("a session")
        .tabs
        .into_iter()
        .map(|tab| tab.note)
        .collect();
    assert_eq!(notes, [Some("b.md".to_owned()), None]);
}
