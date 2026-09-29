use gpui::{WindowHandle, point};

use super::SIDEBAR_X;
use super::quick_open::PICKER_SETTLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

pub(super) fn open(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n\nThe front page.\n")]);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    stage.focus(window);
    stage.keys(window, "cmd-shift-p");
    stage.keys(window, "n");
    stage.wait(PICKER_SETTLE);
    window
}
