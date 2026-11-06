use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Bounds, Focusable, TestAppContext, VisualTestContext, WindowBounds, WindowOptions};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use super::{
    ab_focused, activate, at, check, editor, finish_trash, open, scroll_to, select, showing,
    start_trash, tab_notes, tall_notes, text_of, trash, view_of,
};
use crate::bootstrap::{Launch, root};
use crate::keymap::key_bindings;
use crate::shell::NextTab;
use crate::text_input::InputHost;

#[gpui::test]
fn a_switch_restores_each_tabs_caret_and_top_line(cx: &mut TestAppContext) {
    let (window, mut cx, a, b) = tall_notes(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 610..612);
    let a_top = scroll_to(&window, &mut cx, 600);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    select(&window, &mut cx, 205..205);
    let b_top = scroll_to(&window, &mut cx, 200);
    assert_ne!(a_top, b_top);

    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();
    assert_eq!(view_of(&window, &mut cx), (at(610)..at(612), a_top));

    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();
    assert_eq!(view_of(&window, &mut cx), (at(205)..at(205), b_top));
}

#[gpui::test]
fn switching_back_to_a_note_tab_recreates_the_editor_focused(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    assert!(
        window
            .update(&mut cx, |shell, _, _| shell.editor().is_none())
            .unwrap()
    );

    cx.simulate_keystrokes("ctrl-tab");

    window
        .update(&mut cx, |shell, window, cx| {
            let editor = shell
                .editor()
                .cloned()
                .expect("the note's tab has an editor");
            assert!(editor.focus_handle(cx).is_focused(window));
        })
        .unwrap();
    cx.simulate_input("z");
    assert_eq!(text_of(&window, &mut cx, a), "alphaz");
}

#[gpui::test]
fn typing_in_an_empty_tab_reaches_nothing_and_cmd_p_still_opens(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");

    cx.simulate_input("zz");
    assert_eq!(text_of(&window, &mut cx, a), "alpha");

    cx.simulate_keystrokes("cmd-p");
    let open = window
        .update(&mut cx, |shell, _, cx| shell.quick_open_state(cx).is_some())
        .unwrap();
    assert!(open, "cmd-p reaches the shell from an empty tab");
}

/// No clock advance: only a save scheduled now lands.
#[gpui::test]
fn leaving_a_note_for_an_empty_tab_saves_it_now(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_input("x");

    cx.simulate_keystrokes("cmd-t");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alphax")
    );
}

#[gpui::test]
fn two_tabs_on_one_note_keep_their_own_carets(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    select(&window, &mut cx, 1..1);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    select(&window, &mut cx, 3..3);
    let view = editor(&window, &mut cx).entity_id();

    // A write with no notify: only a rebind's reparse inside the switch would read it.
    window
        .update(&mut cx, |shell, window, cx| {
            let editor = shell.editor().cloned().unwrap();
            let (buffer, parsed_at) = editor.read_with(cx, |editor, _| {
                (editor.buffer().clone(), editor.parsed_at())
            });
            buffer.update(cx, |buffer, _| {
                buffer.transact(|buffer| buffer.insert(buffer.len(), "!"));
            });
            shell.next_tab(&NextTab, window, cx);
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.parsed_at(), parsed_at, "a same-note switch rebound");
                assert_eq!(editor.buffer(), &buffer);
            });
        })
        .unwrap();
    let shown = editor(&window, &mut cx);
    assert_eq!(shown.entity_id(), view, "one note, one editor");
    assert_eq!(shown.update(&mut cx, |editor, _| editor.cursor()), at(1));

    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(
        editor(&window, &mut cx).update(&mut cx, |editor, _| editor.cursor()),
        at(3)
    );
}

/// T5.
#[gpui::test]
fn typing_on_both_sides_of_a_switch_is_two_undo_steps(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    cx.simulate_input("x");
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_input("y");

    cx.simulate_keystrokes("cmd-z");

    assert_eq!(text_of(&window, &mut cx, a), "alphax");
}

#[gpui::test]
fn a_switch_drops_a_composition_and_keeps_its_text(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    window
        .update(&mut cx, |shell, window, cx| {
            let editor = shell.editor().cloned().unwrap();
            editor.update(cx, |editor, cx| {
                editor.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
            });
        })
        .unwrap();

    cx.simulate_keystrokes("ctrl-tab");

    let marked = window
        .update(&mut cx, |shell, window, cx| {
            let editor = shell.editor().cloned().unwrap();
            editor.update(cx, |editor, cx| editor.marked_text_range(window, cx))
        })
        .unwrap();
    assert_eq!(marked, None);
    assert_eq!(text_of(&window, &mut cx, a), "alphaか");
}

#[gpui::test]
fn opening_a_note_replaces_the_active_tabs_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 1));

    cx.simulate_keystrokes("ctrl-tab");
    open(&window, &mut cx, b);
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(b)], 0));
}

/// T4.
#[gpui::test]
fn a_load_landing_after_a_tab_switch_changes_neither_tab(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_load("/vault/b.md");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");

    gate.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), None], 1));
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
}

#[gpui::test]
fn single_file_mode_switches_back_to_its_file(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", "alpha");
    let loaded = futures::executor::block_on(fs.load(Path::new("/notes/a.md"))).unwrap();
    let launch = Launch::Note(PathBuf::from("/notes/a.md"), loaded.text, loaded.stamp);
    let window = cx
        .update(|cx| {
            let bounds = Bounds::maximized(None, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| root(Arc::clone(&fs) as Arc<dyn Fs>, launch, window, cx),
            )
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    window
        .update(&mut cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-t");
    assert_eq!(showing(&window, &mut cx), None);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(showing(&window, &mut cx), Some(EntryId::DETACHED));
    check(&window, &mut cx);
    assert!(
        cx.debug_bounds("dock-right").is_none(),
        "no vault, no backlinks column"
    );

    cx.simulate_input("y");
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    assert_eq!(
        fs.read_to_string(Path::new("/notes/a.md")).as_deref(),
        Some("alphay")
    );
}

/// T1, T3.
#[gpui::test]
fn showing_is_the_active_tabs_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    check(&window, &mut cx);
    open(&window, &mut cx, a);
    check(&window, &mut cx);
    for keys in ["cmd-t", "ctrl-tab", "ctrl-shift-tab"] {
        cx.simulate_keystrokes(keys);
        check(&window, &mut cx);
    }
    open(&window, &mut cx, b);
    check(&window, &mut cx);
    for keys in ["cmd-t", "ctrl-shift-tab", "cmd-}", "cmd-{", "ctrl-tab"] {
        cx.simulate_keystrokes(keys);
        check(&window, &mut cx);
    }
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), None], 2)
    );
}

#[gpui::test]
fn closing_a_note_tab_saves_its_note_now(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_input("x");

    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();

    assert_eq!(
        fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
        Some("alphax")
    );
}

/// The app-level catch for `leaving`: the model's fake has no side effect to seal.
#[gpui::test]
fn closing_an_inactive_tab_does_not_seal_the_active_notes_undo_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_input("x");

    window
        .update(&mut cx, |shell, window, cx| {
            let inactive = shell.center().tabs.id_at(1).unwrap();
            shell.close_tab(inactive, window, cx);
        })
        .unwrap();
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
    cx.simulate_input("y");
    cx.simulate_keystrokes("cmd-z");

    assert_eq!(text_of(&window, &mut cx, a), "alpha");
}

/// M1 in the app: the fresh tab's id is not the one the load was keyed by.
#[gpui::test]
fn a_load_landing_after_its_only_tab_was_closed_lands_nowhere(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, _b) = ab_focused(cx);
    let gate = fs.block_load("/vault/a.md");
    open(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-w");
    gate.release();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx), (vec![None], 0));
    assert!(
        window
            .update(&mut cx, |shell, _, _| shell.editor().is_none())
            .unwrap()
    );
    check(&window, &mut cx);
}

#[gpui::test]
fn cmd_1_on_the_active_tab_seals_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_input("x");
    cx.simulate_keystrokes("cmd-1");
    cx.simulate_input("y");

    cx.simulate_keystrokes("cmd-z");

    assert_eq!(text_of(&window, &mut cx, a), "alpha");
}

#[gpui::test]
fn trashing_a_note_empties_every_tab_showing_it(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);

    trash(&window, &mut cx, a);

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, Some(b), None], 2));
    check(&window, &mut cx);
    window
        .update(&mut cx, |shell, window, _| {
            assert!(
                shell.focus_handle.is_focused(window),
                "the root keeps the keys"
            );
        })
        .unwrap();
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
}

#[gpui::test]
fn trashing_a_note_in_an_inactive_tab_keeps_the_editor(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);
    let view = editor(&window, &mut cx).entity_id();

    trash(&window, &mut cx, a);

    assert_eq!(tab_notes(&window, &mut cx), (vec![None, Some(b)], 1));
    assert_eq!(editor(&window, &mut cx).entity_id(), view);
    check(&window, &mut cx);
}

#[gpui::test]
fn switching_to_a_tab_whose_note_is_being_trashed_shows_it_empty(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    cx.simulate_keystrokes("ctrl-tab");
    let trashing = start_trash(&window, &mut cx, a);

    cx.simulate_keystrokes("ctrl-tab");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(a)], 1));
    assert_eq!(showing(&window, &mut cx), None, "no editor to type into");
    check(&window, &mut cx);
    finish_trash(&fs, gate, trashing, &mut cx);
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), None], 1));
    check(&window, &mut cx);
}

#[gpui::test]
fn going_back_to_a_note_being_trashed_shows_it_empty(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    open(&window, &mut cx, b);
    let trashing = start_trash(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-[");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
    finish_trash(&fs, gate, trashing, &mut cx);
    assert_eq!(tab_notes(&window, &mut cx), (vec![None], 0));
    cx.simulate_keystrokes("cmd-]");
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

#[gpui::test]
fn reopening_a_tab_whose_note_is_being_trashed_shows_it_empty(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, b);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, a);
    let gate = fs.block_save("/vault/a.md");
    cx.simulate_input("x");
    cx.simulate_keystrokes("cmd-w");
    let trashing = start_trash(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-shift-t");

    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), Some(a)], 1));
    assert_eq!(showing(&window, &mut cx), None);
    check(&window, &mut cx);
    finish_trash(&fs, gate, trashing, &mut cx);
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(b), None], 1));
    check(&window, &mut cx);
}

#[gpui::test]
fn a_gone_note_keeps_its_tab(cx: &mut TestAppContext) {
    let (fs, window, mut cx, a, b) = ab_focused(cx);
    open(&window, &mut cx, a);
    cx.simulate_keystrokes("cmd-t");
    open(&window, &mut cx, b);

    fs.remove(Path::new("/vault/a.md"));
    cx.run_until_parked();
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a), Some(b)], 1));
    // Gone from the vault's entries, the title comes from the note's last path.
    let titles = window
        .update(&mut cx, |shell, _, cx| shell.tab_titles(cx))
        .unwrap();
    assert_eq!(titles, ["a", "b"]);

    cx.simulate_keystrokes("ctrl-tab");
    cx.run_until_parked();
    assert_eq!(showing(&window, &mut cx), Some(a));
    assert!(cx.debug_bounds("gone-banner").is_some());
}

/// T6: each key with `⌘P` open closes the picker and still acts.
#[gpui::test]
fn every_tab_key_dismisses_an_open_picker_first(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    for _ in 0..8 {
        cx.simulate_keystrokes("cmd-t");
    }
    let mut steps: Vec<(String, usize, usize)> =
        (1..=8).map(|n| (format!("cmd-{n}"), 9, n - 1)).collect();
    steps.extend(
        [
            ("cmd-9", 9, 8),
            ("ctrl-tab", 9, 0),
            ("ctrl-shift-tab", 9, 8),
            ("cmd-{", 9, 7),
            ("cmd-}", 9, 8),
            ("cmd-[", 9, 8),
            ("cmd-]", 9, 8),
            ("cmd-t", 10, 9),
            ("cmd-w", 9, 8),
            // The tab `cmd-w` closed is empty with no history: nothing to reopen.
            ("cmd-shift-t", 9, 8),
        ]
        .map(|(keys, count, active)| (keys.to_owned(), count, active)),
    );
    assert_eq!(steps.len(), 18);

    for (keys, count, active) in steps {
        cx.simulate_keystrokes("cmd-p");
        assert!(
            window
                .update(&mut cx, |shell, _, _| shell.modal.is_some())
                .unwrap()
        );

        cx.simulate_keystrokes(&keys);

        assert!(
            window
                .update(&mut cx, |shell, _, _| shell.modal.is_none())
                .unwrap(),
            "{keys} left the picker open"
        );
        let (notes, index) = tab_notes(&window, &mut cx);
        assert_eq!((notes.len(), index), (count, active), "{keys}");
    }
}

/// The sidebar opens through `show_note`: a click on the note showing only takes focus.
#[gpui::test]
fn a_sidebar_click_on_the_note_showing_keeps_its_caret_and_scroll(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = tall_notes(cx);
    let sidebar = window
        .update(&mut cx, |shell, _, _| shell.panes().sidebar.clone())
        .unwrap();
    activate(&sidebar, &mut cx, a);
    cx.run_until_parked();
    select(&window, &mut cx, 610..612);
    let top = scroll_to(&window, &mut cx, 600);
    assert_ne!(top, at(0));

    activate(&sidebar, &mut cx, a);
    cx.run_until_parked();

    assert_eq!(view_of(&window, &mut cx), (at(610)..at(612), top));
}

/// `capture` must seal the undo group: the tab shown is on the same note, so no save seals it instead.
#[gpui::test]
fn closing_a_tab_onto_the_same_note_seals_its_undo_group(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    open(&window, &mut cx, a);
    window
        .update(&mut cx, |shell, _, cx| shell.open_in_new_tab(a, None, cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_input("x");
    let typed = text_of(&window, &mut cx, a);

    cx.simulate_keystrokes("cmd-w");
    assert_eq!(tab_notes(&window, &mut cx), (vec![Some(a)], 0));
    cx.simulate_input("y");
    cx.simulate_keystrokes("cmd-z");

    assert_eq!(text_of(&window, &mut cx, a), typed);
}
