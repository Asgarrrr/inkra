use std::time::Duration;

use gpui::{WindowHandle, point, px};
use inkra_layout::Side;

use crate::bootstrap::WINDOW_SIZE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// `index` open, so the right dock lists `reading`.
pub(super) fn default(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[
        ("index.md", "# Index\n\nThe vault's front page.\n"),
        ("reading.md", "Books to read, listed from the [[index]].\n"),
    ]);
    stage.click_tree_row(window, 0.);
    window
}

/// Backlinks moved left by the palette, the pointer on its icon past the tooltip's delay.
pub(super) fn both_left(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = default(stage);
    stage.keys(window, "cmd-shift-p m o v e space b a c k l i n k s enter");
    let icon = stage.dock_icon_point(window, Side::Left, 1);
    stage.hover(window, icon);
    stage.wait(Duration::from_millis(600));
    window
}

/// The Files header's menu at the pointer, the first entry highlighted.
pub(super) fn menu(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = default(stage);
    let header = stage.dock_icon_point(window, Side::Left, 0);
    stage.right_click(window, header);
    stage.keys(window, "down");
    window
}

/// Backlinks' header held mid-height over the left dock's body, the button down.
pub(super) fn drag(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = default(stage);
    let header = stage.dock_icon_point(window, Side::Right, 0);
    let body = point(
        stage.dock_icon_point(window, Side::Left, 0).x,
        WINDOW_SIZE.height / 2.,
    );
    stage.drag_holding(window, header, body);
    window
}

/// Files dragged from the right bar, held at the left edge over its drop strip.
pub(super) fn drag_empty_side(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = default(stage);
    stage.keys(window, "cmd-shift-p m o v e space f i l e s enter");
    let icon = stage.dock_icon_point(window, Side::Right, 1);
    stage.drag_holding(window, icon, point(px(20.), WINDOW_SIZE.height / 2.));
    window
}
