use std::sync::Arc;

use gpui::{Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use super::{activate, open_vault, showing, tree_focused};
use crate::keymap::key_bindings;
use crate::picker::Picker;
use crate::picker::runs::SEARCH_DEBOUNCE;
use crate::quick_open::QuickOpenDelegate;
use crate::shell::{AppShell, NoteStore};
use crate::test_support::entry_id;
use crate::vault_search::VaultSearchDelegate;

/// `a` then `b`, both built into the index; `a` is showing and the editor focused.
fn shown(cx: &mut TestAppContext) -> (WindowHandle<AppShell>, VisualTestContext, [EntryId; 2]) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha plan");
    fs.insert_file("/vault/b.md", "beta\nplan");
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
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

fn modal_is<P: 'static>(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, _| {
            shell
                .modal
                .as_ref()
                .is_some_and(|modal| modal.view().clone().downcast::<P>().is_ok())
        })
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
    assert!(modal_is::<Picker<VaultSearchDelegate>>(window, &mut visual));

    visual.simulate_keystrokes("cmd-shift-f");
    assert!(!has_modal(window, &mut visual));
    assert!(editor_focused(window, &mut visual));
}

#[gpui::test]
fn cmd_shift_f_replaces_quick_open(cx: &mut TestAppContext) {
    let (window, mut visual, _) = shown(cx);
    visual.simulate_keystrokes("cmd-p");
    assert!(modal_is::<Picker<QuickOpenDelegate>>(window, &mut visual));

    visual.simulate_keystrokes("cmd-shift-f");

    assert!(modal_is::<Picker<VaultSearchDelegate>>(window, &mut visual));
    visual.simulate_keystrokes("escape");
    assert!(
        editor_focused(window, &mut visual),
        "the focus before cmd-p"
    );
}

#[gpui::test]
fn cmd_shift_f_does_nothing_without_a_vault(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = cx.update(|cx| {
        gpui::AppContext::new(cx, |cx| NoteStore::new(Arc::new(MemFs::new()), None, cx))
    });
    let window = cx.add_window(move |_, cx| AppShell::new(None, notes, cx));
    window
        .update(cx, |shell, window, cx| {
            window.focus(&shell.focus_handle(cx), cx);
        })
        .expect("the shell is open");
    let mut visual = VisualTestContext::from_window(window.into(), cx);

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
