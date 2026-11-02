use gpui::WindowHandle;

use super::SAMPLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::vault::AUTOSAVE_DELAY;

pub(super) fn save_failed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)]);
    stage.click_tree_row(window, 1.);
    stage.focus(window);
    stage.fs().make_unwritable("/vault/note.md");
    stage.type_text(window, " more");
    stage.wait(AUTOSAVE_DELAY);
    window
}

pub(super) fn open_failed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)]);
    stage.fs().make_unreadable("/vault/note.md");
    stage.click_tree_row(window, 1.);
    window
}

pub(super) fn unwatched(stage: &mut Stage) -> WindowHandle<AppShell> {
    stage.open_unwatched_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)])
}

pub(super) fn create_refused(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)]);
    stage.focus_tree(window);
    stage.fs().make_uncreatable("/vault/plan.md");
    stage.keys(window, "cmd-n p l a n enter");
    window
}
