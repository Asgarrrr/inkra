use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext, px, size};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::{ab_focused, activate, editor, no_vault, open_vault};
use crate::editor::layout::shape_count;
use crate::keymap::key_bindings;
use crate::shell::column::Empty;
use crate::test_support;
use crate::theme;

/// Rooted at `AppShell`: only a flex parent asks the element for its min-content width.
#[gpui::test]
fn a_narrow_pane_is_never_overflowed_by_the_column(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "a long paragraph ".repeat(20));
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = test_support::entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);

    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(700.), px(600.)));
    visual.run_until_parked();

    let bounds = editor(&window, cx).read_with(cx, |view, _| view.text_bounds().unwrap());
    // The editor's `p_4()` leaves 16 px between the column and the window edge.
    assert!(
        bounds.right() <= px(700. - 16.),
        "the column ends at {:?}, past the pane",
        bounds.right()
    );
}

/// Narrower than the column, so taffy's intrinsic-width queries differ from the pane's width.
#[gpui::test]
fn a_keystroke_in_a_narrow_pane_shapes_the_note_once(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "a long paragraph ".repeat(20));
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = test_support::entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(700.), px(600.)));
    visual.simulate_keystrokes("cmd-up");
    visual.run_until_parked();

    let before = shape_count();
    visual.simulate_input("x");
    visual.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

/// The columns end in a hairline against the note, and their content sits inside it.
#[gpui::test]
fn the_chrome_columns_are_ruled_against_the_note(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let sidebar = cx.debug_bounds("dock-left").unwrap();
    let row = cx.debug_bounds("tree-row-0").unwrap();
    assert_eq!(
        row.right(),
        sidebar.right() - theme::RULE_WIDTH - theme::ROW_INSET
    );

    let backlinks = cx.debug_bounds("dock-right").unwrap();
    let header = cx.debug_bounds("panel-header-backlinks").unwrap();
    assert_eq!(header.left(), backlinks.left() + theme::RULE_WIDTH);
}

#[gpui::test]
fn the_empty_state_offers_to_open_a_vault(cx: &mut TestAppContext) {
    let (window, mut visual) = no_vault(cx, &Arc::new(MemFs::new()));

    assert!(visual.debug_bounds("open-vault").is_some());
    let empty = window
        .update(&mut visual, |shell, _, _| shell.empty())
        .unwrap();
    assert_eq!(empty, Empty::NoVault);
}

#[gpui::test]
fn a_vault_shell_offers_no_open_button(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let (window, _sidebar, _notes) = open_vault(cx, fs as Arc<dyn Fs>);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();

    let empty = window
        .update(&mut visual, |shell, _, _| shell.empty())
        .unwrap();
    assert_eq!(empty, Empty::SelectNote);
    assert!(visual.debug_bounds("open-vault").is_none());
}
