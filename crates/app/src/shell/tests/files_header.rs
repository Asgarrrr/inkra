//! The dock views' icons in the title row and the Files header's buttons under it (splits spec, S7).

use gpui::TestAppContext;

use super::docks::inside;
use super::pane_features::switch;
use super::tab_bar::click;
use super::{ab_focused, keys, vault_names};
use crate::shell::docks::MoveFilesToOtherDock;
use crate::theme;

#[gpui::test]
fn dock_icons_sit_in_the_title_row(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);

    for (icon, segment) in [
        ("dock-icon-files", "title-left"),
        ("dock-icon-backlinks", "title-right"),
    ] {
        assert!(inside(&mut cx, icon, segment), "{icon}");
        let bounds = cx.debug_bounds(icon).unwrap();
        assert!(bounds.bottom() <= theme::TITLE_BAR_HEIGHT, "{icon}");
    }
}

#[gpui::test]
fn the_left_toggle_stays_put_when_the_dock_hides(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);
    let shown = cx.debug_bounds("dock-toggle-left").unwrap();

    keys(&mut cx, "cmd-\\");

    assert_eq!(cx.debug_bounds("dock-toggle-left").unwrap(), shown);
}

#[gpui::test]
fn the_files_header_holds_its_buttons_below_the_title_row(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);

    assert!(inside(&mut cx, "panel-header-files", "dock-left"));
    let header = cx.debug_bounds("panel-header-files").unwrap();
    assert_eq!(header.top(), theme::TITLE_BAR_HEIGHT);
    let [note, folder, search] = ["new-note", "new-folder", "search"].map(|button| {
        let selector = format!("sidebar-{button}").leak();
        assert!(inside(&mut cx, selector, "panel-header-files"), "{button}");
        cx.debug_bounds(selector).unwrap().left()
    });
    assert!(note < folder && folder < search);
}

#[gpui::test]
fn the_files_header_new_note_creates_a_note(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);

    assert!(inside(&mut cx, "sidebar-new-note", "panel-header-files"));
    click(&mut cx, "sidebar-new-note");

    let editing = window
        .update(&mut cx, |shell, _, cx| {
            shell.panes().sidebar.read(cx).is_editing()
        })
        .unwrap();
    assert!(editing);
}

#[gpui::test]
fn the_files_header_new_folder_starts_naming_a_folder(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);

    click(&mut cx, "sidebar-new-folder");
    keys(&mut cx, "d o c s enter");

    let notes = window
        .update(&mut cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(
        vault_names(&notes, &mut cx),
        ["docs", "a.md", "b.md"],
        "a folder, with no extension, and sorted before the notes"
    );
}

#[gpui::test]
fn the_files_header_search_opens_vault_search(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, _a, _b) = ab_focused(cx);

    assert!(inside(&mut cx, "sidebar-search", "panel-header-files"));
    click(&mut cx, "sidebar-search");

    let open = window
        .update(&mut cx, |shell, _, _| {
            shell.modal_is::<crate::vault_search::VaultSearchDelegate>()
        })
        .unwrap();
    assert!(open);
}

#[gpui::test]
fn the_files_header_search_follows_the_vault_search_switch(cx: &mut TestAppContext) {
    let (fs, window, mut cx, _a, _b) = ab_focused(cx);

    switch(&fs, "vault_search", false, &mut cx);

    assert!(cx.debug_bounds("sidebar-search").is_none());
    for button in ["sidebar-new-note", "sidebar-new-folder"] {
        assert!(inside(&mut cx, button, "panel-header-files"), "{button}");
    }

    switch(&fs, "vault_search", true, &mut cx);

    assert!(inside(&mut cx, "sidebar-search", "panel-header-files"));
    click(&mut cx, "sidebar-search");
    let open = window
        .update(&mut cx, |shell, _, _| {
            shell.modal_is::<crate::vault_search::VaultSearchDelegate>()
        })
        .unwrap();
    assert!(open);
}

#[gpui::test]
fn the_files_header_moves_with_files(cx: &mut TestAppContext) {
    let (_fs, _window, mut cx, _a, _b) = ab_focused(cx);

    cx.dispatch_action(MoveFilesToOtherDock);
    cx.run_until_parked();

    assert!(inside(&mut cx, "panel-header-files", "dock-right"));
}
