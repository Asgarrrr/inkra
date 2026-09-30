use gpui::{WindowHandle, point};

use super::{LINKED, SIDEBAR_X};
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

pub(super) fn bar(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    stage.keys(window, "cmd-t");
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    stage.keys(window, "cmd-t cmd-t cmd-1");
    window
}

pub(super) fn dragging(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    let (from, to) = (stage.tab_center(window, 0), stage.tab_center(window, 2));
    stage.drag_holding(window, from, to);
    window
}
