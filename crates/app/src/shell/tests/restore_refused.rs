//! A saved session the restore refuses: the user already holds a working set, the walk failed, or the file is bad (restore-tabs spec, R6).

use std::path::Path;

use gpui::TestAppContext;

use super::restore_lands::{empty, id, launch, notes, notes_group, row, save, save_tree, vault};
use super::session_writes::{SESSION, saved_tabs, tab, wait};
use super::splits::centre;
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
    let saved: Vec<_> = saved_tabs(&fs)
        .unwrap()
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
    let saved: Vec<_> = saved_tabs(&fs)
        .unwrap()
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

/// Quit while the restore's own load of a.md is gated: `Landing` must not write.
#[gpui::test]
fn a_quit_while_the_restore_lands_writes_nothing(cx: &mut TestAppContext) {
    let fs = vault();
    let text = save(&fs, 0, vec![tab("a.md", 0, 0)]);
    let walk = fs.block_read_dir("/vault/f");
    let (_window, cx) = launch(cx, &fs);
    cx.run_until_parked();
    walk.release();
    let a = Path::new("/vault/a.md");
    // The index loads a.md first; gate the second load, the restore's.
    let mut gate = None;
    while gate.is_none() {
        if fs.loads(a) >= 1 {
            gate = Some(fs.block_load(a));
        } else {
            assert!(cx.executor().tick());
        }
    }
    cx.run_until_parked();

    cx.quit();

    assert_eq!(fs.read_to_string(Path::new(SESSION)), Some(text));
}

/// Done from the start; the walk's notify arms the throttle.
#[gpui::test]
fn a_corrupt_file_restores_nothing(cx: &mut TestAppContext) {
    let fs = vault();
    fs.insert_file(SESSION, "{\"version\": 1, \"tabs\": [");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    assert_eq!(notes(&window, &mut cx), (vec![None], 0));

    wait(&mut cx, THROTTLE);

    assert_eq!(saved_tabs(&fs).unwrap(), [empty()]);
}

#[gpui::test]
fn a_refused_restore_leaves_no_empty_group(cx: &mut TestAppContext) {
    let fs = vault();
    let center = row(vec![
        notes_group(vec![tab("b.md", 0, 0)], 0),
        notes_group(vec![tab("f/c.md", 0, 0)], 0),
        notes_group(vec![tab("b.md", 0, 0)], 0),
    ]);
    save_tree(&fs, 2, center);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut cx) = launch(cx, &fs);
    cx.run_until_parked();
    assert_eq!(
        centre(&window, &mut cx).len(),
        3,
        "the tree draws before the walk"
    );

    let a = id(&window, &mut cx, "a");
    window
        .update(&mut cx, |shell, _, cx| shell.open_note(a, None, cx))
        .unwrap();
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert_eq!(centre(&window, &mut cx).len(), 1);
    assert_eq!(notes(&window, &mut cx), (vec![Some(a)], 0));
    wait(&mut cx, THROTTLE);
    let saved: Vec<_> = saved_tabs(&fs)
        .unwrap()
        .into_iter()
        .map(|tab| tab.note)
        .collect();
    assert_eq!(saved, [Some("a.md".to_owned())]);
}
