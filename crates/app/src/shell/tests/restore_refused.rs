//! A saved session the restore refuses: the user already holds a working set, the walk failed, or the file is bad (restore-tabs spec, R6).

use std::path::Path;

use gpui::TestAppContext;

use super::restore_lands::{empty, id, launch, notes, save, vault};
use super::session_writes::{SESSION, saved, tab, wait};
use crate::open::{NoteActivated, Open};
use crate::shell::session::THROTTLE;

#[gpui::test]
fn a_sidebar_open_before_the_walk_wins(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("f/c.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    let sidebar = window
        .update(&mut cx, |shell, _, _| shell.panes().sidebar.clone())
        .unwrap();
    sidebar.update(&mut cx, |_, cx| {
        cx.emit(NoteActivated {
            note: a,
            open: Open::Here,
        });
    });
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    wait(&mut cx, THROTTLE);
    let saved: Vec<_> = saved(&fs)
        .unwrap()
        .tabs
        .into_iter()
        .map(|tab| tab.note)
        .collect();
    assert_eq!(saved, [Some("a.md".to_owned())]);
}

/// Checked before the loads: a refused restore opens none of its notes.
#[gpui::test]
fn a_refused_restore_opens_no_saved_note(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("f/c.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    window
        .update(&mut cx, |shell, _, cx| shell.open_note(a, None, cx))
        .unwrap();
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let c = id(&window, &mut cx, "c");
    let opened = window
        .update(&mut cx, |shell, _, cx| {
            shell.notes.read(cx).buffer_for(c).is_some()
        })
        .unwrap();
    assert!(!opened, "c.md was loaded for a restore already refused");
}

#[gpui::test]
fn a_new_tab_before_the_walk_wins(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.simulate_keystrokes("cmd-t");
    gate.release();
    cx.run_until_parked();

    assert_eq!(notes(&window, &mut cx), (vec![None, None], 1));
}

/// The user's open is pending when the restore lands: the restore must neither cancel it nor apply.
#[gpui::test]
fn a_user_open_pending_when_the_restore_lands_wins(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("a.md", 0, 0), tab("f/c.md", 0, 0)]);
    let walk = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let b = id(&window, &mut cx, "b");
    let load = fs.block_load("/vault/b.md");
    window
        .update(&mut cx, |shell, _, cx| shell.open_note(b, None, cx))
        .unwrap();
    walk.release();
    cx.run_until_parked();
    load.release();
    cx.run_until_parked();

    assert_eq!(notes(&window, &mut cx), (vec![Some(b)], 0));
    wait(&mut cx, THROTTLE);
    let saved: Vec<_> = saved(&fs)
        .unwrap()
        .tabs
        .into_iter()
        .map(|tab| tab.note)
        .collect();
    assert_eq!(
        saved,
        [Some("b.md".to_owned())],
        "the restore ended, so writes resumed"
    );
}

/// Back leaves no open pending, yet the note it shows is the user's.
#[gpui::test]
fn a_note_shown_by_back_before_the_walk_wins(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("f/c.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let (a, b) = (id(&window, &mut cx, "a"), id(&window, &mut cx, "b"));
    for note in [a, b] {
        window
            .update(&mut cx, |shell, _, cx| shell.open_note(note, None, cx))
            .unwrap();
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("cmd-[");
    gate.release();
    cx.run_until_parked();

    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
}

#[gpui::test]
fn a_tab_closed_before_the_walk_wins(cx: &mut TestAppContext) {
    let fs = vault();
    save(&fs, 0, vec![tab("b.md", 0, 0)]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();

    let a = id(&window, &mut cx, "a");
    window
        .update(&mut cx, |shell, _, cx| shell.open_note(a, None, cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-w");
    gate.release();
    cx.run_until_parked();

    assert_eq!(notes(&window, &mut cx), (vec![None], 0));
}

/// A failed walk sets `is_scanned` too; resolving on it would drop every tab, then overwrite the file.
#[gpui::test]
fn a_failed_walk_restores_nothing_and_writes_nothing(cx: &mut TestAppContext) {
    let fs = vault();
    let text = save(&fs, 0, vec![tab("a.md", 0, 0)]);
    fs.make_unreadable("/vault");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-t");
    wait(&mut cx, THROTTLE);

    cx.quit();

    assert_eq!(fs.read_to_string(Path::new(SESSION)), Some(text));
    let _ = window;
}

#[gpui::test]
fn a_quit_before_the_restore_lands_writes_nothing(cx: &mut TestAppContext) {
    let fs = vault();
    let text = save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let _gate = fs.block_load("/vault/a.md");
    let (_window, cx) = launch(cx, &fs);
    cx.run_until_parked();

    cx.quit();

    assert_eq!(fs.read_to_string(Path::new(SESSION)), Some(text));
}

/// Read after the walk, so no later change arms the throttle: the restore's end must, and the write replaces the file.
#[gpui::test]
fn a_corrupt_file_restores_nothing(cx: &mut TestAppContext) {
    let fs = vault();
    fs.insert_file(SESSION, "{\"version\": 1, \"tabs\": [");
    let read = fs.block_load(SESSION);
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    wait(&mut cx, THROTTLE);
    read.release();
    cx.run_until_parked();
    assert_eq!(notes(&window, &mut cx), (vec![None], 0));

    wait(&mut cx, THROTTLE);

    assert_eq!(saved(&fs).unwrap().tabs, [empty()]);
}
