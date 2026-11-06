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
