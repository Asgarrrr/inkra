use gpui::WindowHandle;

use super::{VAULT, quick_open};
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// `projects` expanded and selected; the pointer on `index`.
pub(super) fn sidebar(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click_tree_row(window, 0.);
    stage.hover_tree_row(window, 3.);
    window
}

pub(super) fn sidebar_selected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click_tree_row(window, 1.);
    stage.hover_tree_row(window, 1.);
    window
}

pub(super) fn picker(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = quick_open::results(stage);
    let row = stage.quick_open_row_center(window, 2.);
    stage.hover(window, row);
    window
}
