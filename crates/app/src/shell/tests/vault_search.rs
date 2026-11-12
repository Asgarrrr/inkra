use std::fmt::Write as _;
use std::ops::Range;
use std::sync::Arc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, WindowHandle, px};
use inkra_text::ByteOffset;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use super::{activate, editor, no_vault, open_vault, showing, tree_focused};
use crate::keymap::key_bindings;
use crate::open::Open;
use crate::picker::runs::SEARCH_DEBOUNCE;
use crate::picker::{Picker, PickerDelegate};
use crate::quick_open::QuickOpenDelegate;
use crate::shell::AppShell;
use crate::test_support::entry_id;
use crate::vault_search::{HitActivated, VaultSearchDelegate};

/// `a` then `b`, both built into the index; `a` is showing and the editor focused.
fn shown(cx: &mut TestAppContext) -> (WindowHandle<AppShell>, VisualTestContext, [EntryId; 2]) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha plan");
    fs.insert_file("/vault/b.md", "beta\nplan");
    shown_in(cx, &fs)
}

fn shown_in(
    cx: &mut TestAppContext,
    fs: &Arc<MemFs>,
) -> (WindowHandle<AppShell>, VisualTestContext, [EntryId; 2]) {
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let ids = ["a", "b"].map(|name| entry_id(&notes, cx, name));
    activate(&sidebar, cx, ids[0]);
    cx.run_until_parked();
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("the shell is open");
    visual.run_until_parked();
    (window, visual, ids)
}

fn modal_is<D: PickerDelegate>(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.modal_is::<D>())
        .expect("the shell is open")
}

fn has_modal(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| shell.modal.is_some())
        .expect("the shell is open")
}

fn editor_focused(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            shell.focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open")
}

fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
}

#[gpui::test]
fn cmd_shift_f_opens_vault_search_and_again_closes_it(cx: &mut TestAppContext) {
    let (window, mut visual, _) = shown(cx);

    visual.simulate_keystrokes("cmd-shift-f");
    assert!(modal_is::<VaultSearchDelegate>(window, &mut visual));

    visual.simulate_keystrokes("cmd-shift-f");
    assert!(!has_modal(window, &mut visual));
    assert!(editor_focused(window, &mut visual));
}

#[gpui::test]
fn cmd_shift_f_replaces_quick_open(cx: &mut TestAppContext) {
    let (window, mut visual, _) = shown(cx);
    visual.simulate_keystrokes("cmd-p");
    assert!(modal_is::<QuickOpenDelegate>(window, &mut visual));

    visual.simulate_keystrokes("cmd-shift-f");

    assert!(modal_is::<VaultSearchDelegate>(window, &mut visual));
    visual.simulate_keystrokes("escape");
    assert!(
        editor_focused(window, &mut visual),
        "the focus before cmd-p"
    );
}

#[gpui::test]
fn cmd_shift_f_does_nothing_without_a_vault(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));

    visual.simulate_keystrokes("cmd-shift-f");

    assert!(!has_modal(window, &mut visual));
}

#[gpui::test]
fn enter_opens_the_note_of_the_selected_hit(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b]) = shown(cx);
    visual.simulate_keystrokes("cmd-shift-f");
    visual.simulate_keystrokes("p l a n");
    settle(&mut visual);

    // a's header, a's hit, b's header, b's hit.
    visual.simulate_keystrokes("down enter");

    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(!has_modal(window, &mut visual));
    assert!(editor_focused(window, &mut visual));
}

#[gpui::test]
fn enter_typed_before_the_results_land_opens_the_first_hit(cx: &mut TestAppContext) {
    let (window, mut visual, [_a, b]) = shown(cx);
    visual.simulate_keystrokes("cmd-shift-f");

    // `b` answers at once; `e t a` wait behind it, and `enter` waits for them.
    visual.simulate_keystrokes("b e t a enter");
    assert!(
        has_modal(window, &mut visual),
        "the results have not landed"
    );
    settle(&mut visual);

    assert_eq!(showing(&window, &mut visual), Some(b));
    assert!(!has_modal(window, &mut visual));
}

#[gpui::test]
fn from_the_tree_escape_dismisses_and_the_tree_keeps_focus(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, _notes, a, _b) = super::two_notes(cx, &fs);
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = tree_focused(cx, window, &sidebar);

    visual.simulate_keystrokes("cmd-shift-f escape");

    assert!(!has_modal(window, &mut visual));
    let tree_focused = window
        .update(&mut visual, |shell, window, cx| {
            shell.tree_focus_handle(cx).is_focused(window)
        })
        .expect("the shell is open");
    assert!(tree_focused);
}

/// 300 lines; only line `hit` says `plan`.
fn long_note(hit: usize) -> String {
    (1..=300).fold(String::new(), |mut text, line| {
        let word = if line == hit { "plan" } else { "rest" };
        let _ = writeln!(text, "Week {line}: {word}");
        text
    })
}

fn range_of(text: &str, word: &str) -> Range<ByteOffset> {
    let start = text.find(word).expect("the word is in the text");
    ByteOffset::new(start)..ByteOffset::new(start + word.len())
}

fn vault_search(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<VaultSearchDelegate>> {
    window
        .update(cx, |shell, _, _| shell.modal_picker())
        .expect("the shell is open")
        .expect("vault search is open")
}

#[gpui::test]
fn a_hit_far_down_a_long_note_opens_selected_and_in_view(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let long = long_note(250);
    fs.insert_file("/vault/a.md", "alpha plan");
    fs.insert_file("/vault/b.md", long.as_str());
    let (window, mut visual, [_a, b]) = shown_in(cx, &fs);

    visual.simulate_keystrokes("cmd-shift-f p l a n");
    settle(&mut visual);
    visual.simulate_keystrokes("down enter");

    assert_eq!(showing(&window, &mut visual), Some(b));
    editor(&window, &mut visual).update(&mut visual, |editor, _| {
        assert_eq!(editor.selected(), range_of(&long, "plan"));
        assert!(editor.scroll_y() < px(0.), "the note opened at its top");
        assert!(editor.caret_in_view(), "the caret is outside the viewport");
    });
    assert!(editor_focused(window, &mut visual));
}

/// Rebinding would reset the undo stack with the buffer, and the caret and scroll with it.
#[gpui::test]
fn a_hit_in_the_note_showing_selects_without_rebinding(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    let long = long_note(20);
    fs.insert_file("/vault/a.md", long.as_str());
    fs.insert_file("/vault/b.md", "beta");
    let (window, mut visual, [a, _b]) = shown_in(cx, &fs);
    // Unsaved, and after the hit, so the saved range still fits.
    visual.simulate_keystrokes("cmd-down x");
    let before =
        editor(&window, &mut visual).update(&mut visual, |editor, _| editor.buffer().entity_id());

    visual.simulate_keystrokes("cmd-shift-f p l a n");
    settle(&mut visual);
    visual.simulate_keystrokes("enter");

    assert_eq!(showing(&window, &mut visual), Some(a));
    editor(&window, &mut visual).update(&mut visual, |editor, _| {
        assert_eq!(editor.buffer().entity_id(), before);
        assert_eq!(editor.selected(), range_of(&long, "plan"));
        assert!(editor.caret_in_view(), "the caret is outside the viewport");
    });
    assert!(editor_focused(window, &mut visual));
    visual.simulate_keystrokes("cmd-z");
    editor(&window, &mut visual).update(&mut visual, |editor, cx| {
        assert_eq!(
            editor.buffer().read(cx).text(),
            long,
            "the edit is still undoable"
        );
    });
}

/// The index's range is in the saved text; the buffer may be shorter by now.
#[gpui::test]
fn a_hit_past_the_end_of_an_edited_note_is_clipped(cx: &mut TestAppContext) {
    let (window, mut visual, [a, _b]) = shown(cx);
    // `alpha plan` becomes `alpha pl`, unsaved.
    visual.simulate_keystrokes("cmd-down backspace backspace");
    visual.simulate_keystrokes("cmd-shift-f");

    let picker = vault_search(window, &mut visual);
    picker.update(&mut visual, |_, cx| {
        cx.emit(HitActivated {
            note: a,
            range: ByteOffset::new(6)..ByteOffset::new(10),
            open: Open::Here,
        });
    });
    visual.run_until_parked();

    editor(&window, &mut visual).update(&mut visual, |editor, _| {
        assert_eq!(editor.selected(), ByteOffset::new(6)..ByteOffset::new(8));
    });
}

#[gpui::test]
fn picking_a_hit_then_another_drops_the_first(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha plan");
    fs.insert_file("/vault/b.md", "beta\nplan");
    let (window, mut visual, [a, _b]) = shown_in(cx, &fs);

    visual.simulate_keystrokes("cmd-shift-f p l a n");
    settle(&mut visual);
    let gate = fs.block_load("/vault/b.md");
    visual.simulate_keystrokes("down enter");
    visual.simulate_keystrokes("cmd-shift-f p l a n");
    settle(&mut visual);
    visual.simulate_keystrokes("enter");
    gate.release();
    visual.run_until_parked();

    assert_eq!(showing(&window, &mut visual), Some(a));
    editor(&window, &mut visual).update(&mut visual, |editor, _| {
        assert_eq!(editor.selected(), range_of("alpha plan", "plan"));
    });
}
