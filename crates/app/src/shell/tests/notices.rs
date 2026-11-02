//! The one line reporting a failure: failed saves, renames and opens, and a preserved conflict (quit-and-failures spec, Q9).

use std::io::ErrorKind;

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;

use super::session_writes::wait;
use super::{ab_focused, open};
use crate::shell::AppShell;
use crate::test_support::conflicts;
use crate::vault::AUTOSAVE_DELAY;

fn notice(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |shell, _, _| {
            shell.notice.as_ref().map(|notice| notice.text.to_string())
        })
        .unwrap()
}

/// `a` open and edited, its save failing for good.
fn failed_save(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<AppShell>,
    VisualTestContext,
    std::sync::Arc<MemFs>,
) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    fs.make_unwritable("/vault/a.md");
    open(&window, &mut visual, a);
    visual.simulate_input("x");
    wait(&mut visual, AUTOSAVE_DELAY);
    (window, visual, fs)
}

#[gpui::test]
fn a_failed_save_shows_a_notice_with_the_note_and_the_reason(cx: &mut TestAppContext) {
    let (window, mut visual, _fs) = failed_save(cx);

    assert_eq!(
        notice(&window, &mut visual).as_deref(),
        Some("Couldn't save “a”: permission denied")
    );
    assert!(visual.debug_bounds("notice").is_some());
}

#[gpui::test]
fn the_dismiss_button_clears_the_notice(cx: &mut TestAppContext) {
    let (window, mut visual, _fs) = failed_save(cx);
    let button = visual
        .debug_bounds("notice-dismiss")
        .expect("the notice has a dismiss button");

    visual.simulate_click(button.center(), Modifiers::none());
    visual.run_until_parked();

    assert_eq!(notice(&window, &mut visual), None);
    assert_eq!(visual.debug_bounds("notice"), None);
}

#[gpui::test]
fn the_next_landed_save_of_that_note_clears_its_notice(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    fs.fail_next_saves("/vault/a.md", 1, ErrorKind::PermissionDenied);
    open(&window, &mut visual, a);
    visual.simulate_input("x");
    wait(&mut visual, AUTOSAVE_DELAY);
    assert!(notice(&window, &mut visual).is_some(), "the save failed");

    visual.simulate_input("y");
    wait(&mut visual, AUTOSAVE_DELAY);

    assert_eq!(notice(&window, &mut visual), None);
}

#[gpui::test]
async fn a_failed_rename_shows_a_notice(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    fs.make_uncreatable("/vault/z.md");
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();

    let renamed = notes.update(&mut visual, |notes, cx| notes.rename(a, "z", cx));
    visual.run_until_parked();
    assert!(!renamed.await);

    assert_eq!(
        notice(&window, &mut visual).as_deref(),
        Some("Couldn't rename “a”: permission denied")
    );
}

#[gpui::test]
fn a_preserved_conflict_shows_a_notice_naming_the_copy(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    let (preserved, _guard) = conflicts(&notes, cx);
    open(&window, &mut visual, a);
    fs.insert_file("/vault/a.md", "theirs");

    visual.simulate_input("x");
    wait(&mut visual, AUTOSAVE_DELAY);

    visual.simulate_input("y");
    wait(&mut visual, AUTOSAVE_DELAY);

    let copy = preserved.borrow()[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        notice(&window, &mut visual),
        Some(format!(
            "“a” changed on disk while you edited it. The disk version is kept as “{copy}”."
        ))
    );
}

#[gpui::test]
fn a_note_that_cannot_be_read_shows_a_notice(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    fs.make_unreadable("/vault/a.md");

    open(&window, &mut visual, a);

    assert_eq!(
        notice(&window, &mut visual).as_deref(),
        Some("Couldn't open “a”: permission denied")
    );
}
