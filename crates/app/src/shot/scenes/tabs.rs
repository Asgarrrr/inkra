use gpui::{WindowHandle, point, px};

use super::LINKED;
use crate::bootstrap::WINDOW_SIZE;
use crate::restore::{SavedFileTab, SavedView};
use crate::shell::AppShell;
use crate::shot::stage::Stage;

pub(super) fn bar(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click_tree_row(window, 0.);
    stage.keys(window, "cmd-t");
    stage.click_tree_row(window, 1.);
    stage.keys(window, "cmd-t cmd-t cmd-1");
    window
}

/// `index` typed into: the active tab carries the dot, since a note left saves at once (`set_current`).
pub(super) fn dirty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.keys(window, "x");
    window
}

pub(super) fn hover(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    let reading = stage.tab_center(window, 1);
    stage.hover(window, reading);
    window
}

pub(super) fn panels_hidden(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.keys(window, "cmd-\\ cmd-|");
    window
}

pub(super) fn toggle_hover(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.hover(window, AppShell::left_toggle_point());
    window
}

/// Thirty tabs: the bar scrolls, and the drag region keeps its floor.
pub(super) fn overflow(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.keys(window, &format!("{}cmd-9", "cmd-t ".repeat(26)));
    window
}

pub(super) fn dragging(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    let (from, to) = (stage.tab_center(window, 0), stage.tab_center(window, 2));
    stage.drag_holding(window, from, to);
    window
}

/// `reading`'s tab held 30 px inside the note column's right edge, left of the 260 px right dock.
pub(super) fn drop_split(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    let from = stage.tab_center(window, 1);
    let edge = point(WINDOW_SIZE.width - px(290.), WINDOW_SIZE.height / 2.);
    stage.drag_holding(window, from, edge);
    window
}

fn saved(note: &str, tail: usize, head: usize) -> SavedFileTab {
    SavedFileTab {
        file: Some(note.to_owned()),
        view: Some(SavedView {
            tail,
            head,
            top: 0,
            below: 0.,
        }),
    }
}

pub(super) fn restored(stage: &mut Stage) -> WindowHandle<AppShell> {
    let tabs = vec![
        saved("index.md", 2, 2),
        saved("reading.md", 0, 14),
        saved("today.md", 8, 8),
    ];
    let window = stage.open_restored(LINKED, tabs, 1);
    stage.focus(window);
    window
}

pub(super) fn nav_arrows(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.click_tree_row(window, 2.);
    window
}

/// `nav_arrows`, then another note and a back: the back arrow's menu holds a forward row, the current row and a back row.
pub(super) fn history_menu(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.click_tree_row(window, 2.);
    stage.click_tree_row(window, 1.);
    stage.keys(window, "cmd-[");
    let arrow = stage.nav_back_center(window);
    stage.right_click(window, arrow);
    window
}
