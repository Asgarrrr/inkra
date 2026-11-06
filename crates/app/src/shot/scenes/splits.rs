use gpui::WindowHandle;

use super::LINKED;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// `index` split right by the palette, then `reading` opened from the tree into the right group.
pub(super) fn right(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click_tree_row(window, 0.);
    stage.keys(window, "cmd-shift-p s p l i t space r i g h t enter");
    stage.click_tree_row(window, 1.);
    window
}

/// `right`, then the right group's tab right-clicked: its menu at the pointer, the first entry highlighted.
pub(super) fn tab_menu(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = right(stage);
    let tab = stage.tab_center(window, 0);
    stage.right_click(window, tab);
    stage.keys(window, "down");
    window
}

/// `index` split down by the palette, then `reading` opened from the tree into the lower group.
pub(super) fn down(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click_tree_row(window, 0.);
    stage.keys(window, "cmd-shift-p s p l i t space d o w n enter");
    stage.click_tree_row(window, 1.);
    window
}

/// `right`, then the right group split down by the palette.
pub(super) fn grid(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = right(stage);
    stage.keys(window, "cmd-shift-p s p l i t space d o w n enter");
    window
}
