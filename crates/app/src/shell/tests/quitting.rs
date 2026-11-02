//! Close is quit: every unsaved note and the session are written before the app exits (quit spec, Q1–Q2).

use std::path::Path;
use std::time::Duration;

use gpui::TestAppContext;

use super::session_writes::{saved, saving, tab, wait};
use super::{ab_focused, open};
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
fn a_hung_save_holds_the_quit_no_longer_than_the_quit_wait(cx: &mut TestAppContext) {
    let (fs, window, mut visual, a, _b) = saving(cx);
    open(&window, &mut visual, a);
    let gate = fs.block_save("/vault/a.md");
    visual.simulate_input("x");

    let quit = window
        .update(&mut visual, |shell, window, cx| {
            shell.begin_quit(window, cx)
        })
        .unwrap()
        .expect("the first quit");
    visual.run_until_parked();

    let quitting = window
        .update(&mut visual, |shell, _, _| shell.quitting)
        .unwrap();
    assert!(quitting, "the guard is set");
    assert_eq!(
        saved(&fs).expect("the quit's session").tabs,
        [tab("a.md", 6, 6)]
    );
    wait(
        &mut visual,
        QUIT_WAIT.saturating_sub(Duration::from_millis(1)),
    );
    assert!(!quit.is_ready(), "the quit waits on the hung save");
    wait(&mut visual, Duration::from_millis(1));
    assert!(quit.is_ready(), "the quit runs once the wait ends");
    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alpha"));

    gate.release();
    visual.run_until_parked();
    assert_eq!(on_disk(&fs, "/vault/a.md").as_deref(), Some("alphax"));
}
