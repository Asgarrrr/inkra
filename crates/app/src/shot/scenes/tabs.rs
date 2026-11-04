use gpui::WindowHandle;

use super::LINKED;
use crate::restore::{SavedTab, SavedView};
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
    stage.hover(window, AppShell::sidebar_toggle_point());
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

fn saved(note: &str, tail: usize, head: usize) -> SavedTab {
    SavedTab {
        note: Some(note.to_owned()),
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

pub(super) fn nav_row(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = bar(stage);
    stage.click_tree_row(window, 2.);
    window
}
