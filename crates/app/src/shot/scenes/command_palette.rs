use gpui::WindowHandle;

use super::quick_open::PICKER_SETTLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

pub(super) fn open(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n\nThe front page.\n")]);
    stage.click_tree_row(window, 0.);
    stage.focus(window);
    stage.keys(window, "cmd-shift-p");
    stage.keys(window, "n");
    stage.wait(PICKER_SETTLE);
    window
}
