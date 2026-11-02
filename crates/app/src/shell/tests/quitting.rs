//! Close is quit: every unsaved note and the session are written before the app exits, or the user is asked (quit spec, Q1–Q5).

use std::path::Path;
use std::time::Duration;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};

use super::session_writes::{saved, saving, tab, wait};
use super::{ab_focused, open};
use crate::shell::AppShell;
use crate::shell::quit::QUIT_WAIT;

fn on_disk(fs: &inkra_vault::mem_fs::MemFs, path: &str) -> Option<String> {
    fs.read_to_string(Path::new(path))
}

#[gpui::test]
fn closing_the_window_saves_the_notes_and_the_session_then_quits(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    open(&window, &mut visual, a);
    visual.simulate_input("x");

    assert!(!visual.simulate_close(), "the quit closes the window");
    visual.run_until_parked();

    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
    assert_eq!(
        saved(&fs).expect("the quit's session").tabs,
        [tab("a.md", 6, 6)]
    );
}

#[gpui::test]
fn cmd_q_saves_the_notes_before_quitting(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    open(&window, &mut visual, a);
    visual.simulate_input("x");

    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();

    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
}

/// Chained, not `flush_all`: an off-chain write would resolve the stale `a.md` and recreate it.
#[gpui::test]
fn a_quit_behind_a_rename_writes_the_new_name_only(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    open(&window, &mut visual, a);
    let notes = window
        .update(&mut visual, |shell, _, _| shell.notes.clone())
        .unwrap();
    let gate = fs.block_read_dir("/vault");
    let _renamed = notes.update(&mut visual, |notes, cx| notes.rename(a, "z", cx));
    visual.run_until_parked();

    visual.simulate_input("x");
    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();
    gate.release();
    visual.run_until_parked();

    assert_eq!(on_disk(&fs, "/vault/z.md").as_deref(), Some("alphax"));
    assert_eq!(on_disk(&fs, "/vault/a.md"), None);
}

/// The second `⌘Q` would preempt the "y" autosave, still on its timer, and write it.
#[gpui::test]
fn a_second_cmd_q_while_quitting_starts_no_second_flush(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    open(&window, &mut visual, a);
    let gate = fs.block_save("/vault/a.md");
    visual.simulate_input("x");
    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();

    visual.simulate_input("y");
    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();
    gate.release();
    visual.run_until_parked();

    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
}

/// The wait bounds the quit, not the write: the save still lands once its disk answers.
#[gpui::test]
fn a_hung_save_asks_only_after_the_quit_wait(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    open(&window, &mut visual, a);
    let gate = fs.block_save("/vault/a.md");
    visual.simulate_input("x");

    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();

    assert!(quitting(&window, &mut visual), "the guard is set");
    assert_eq!(
        saved(&fs).expect("the quit's session").tabs,
        [tab("a.md", 6, 6)]
    );
    assert_eq!(
        visual.pending_prompt(),
        None,
        "the quit waits on the hung save"
    );
    wait(
        &mut visual,
        QUIT_WAIT.saturating_sub(Duration::from_millis(1)),
    );
    assert_eq!(
        visual.pending_prompt(),
        None,
        "the quit waits on the hung save"
    );
    wait(&mut visual, Duration::from_millis(1));
    assert_eq!(
        visual.pending_prompt(),
        Some(unsaved_a()),
        "it asks once the wait ends"
    );
    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alpha"));

    gate.release();
    visual.run_until_parked();
    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
}

fn quitting(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window.update(cx, |shell, _, _| shell.quitting).unwrap()
}

fn unsaved_a() -> (String, String) {
    (
        "Some changes are not saved.".into(),
        "Not on disk: “a”.".into(),
    )
}

/// `a` edited and unwritable, then `⌘Q` pressed `presses` times.
fn quit_unwritable(
    cx: &mut TestAppContext,
    presses: usize,
) -> (WindowHandle<AppShell>, VisualTestContext) {
    let (fs, window, mut visual, a, _b) = ab_focused(cx);
    fs.make_unwritable("/vault/a.md");
    open(&window, &mut visual, a);
    visual.simulate_input("x");
    for _ in 0..presses {
        visual.simulate_keystrokes("cmd-q");
    }
    visual.run_until_parked();
    (window, visual)
}

#[gpui::test]
fn a_note_that_cannot_be_written_asks_before_quitting(cx: &mut TestAppContext) {
    let (_window, visual) = quit_unwritable(cx, 1);

    assert_eq!(visual.pending_prompt(), Some(unsaved_a()));
}

#[gpui::test]
fn cancel_keeps_the_app_open_and_the_next_quit_asks_again(cx: &mut TestAppContext) {
    let (window, mut visual) = quit_unwritable(cx, 1);

    visual.simulate_prompt_answer("Cancel");
    visual.run_until_parked();
    assert_eq!(visual.pending_prompt(), None);
    assert!(!quitting(&window, &mut visual), "Cancel resets the guard");

    visual.simulate_keystrokes("cmd-q");
    visual.run_until_parked();
    assert_eq!(visual.pending_prompt(), Some(unsaved_a()));
}

#[gpui::test]
fn a_second_cmd_q_while_quitting_opens_no_second_prompt(cx: &mut TestAppContext) {
    let (_window, visual) = quit_unwritable(cx, 2);

    visual.simulate_prompt_answer("Cancel");
    visual.run_until_parked();

    assert_eq!(visual.pending_prompt(), None);
}

#[gpui::test]
fn quit_anyway_keeps_the_guard_set(cx: &mut TestAppContext) {
    let (window, mut visual) = quit_unwritable(cx, 1);

    visual.simulate_prompt_answer("Quit Anyway");
    visual.run_until_parked();

    assert!(quitting(&window, &mut visual), "the app is quitting");
}
