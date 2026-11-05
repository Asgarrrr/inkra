use gpui::WindowHandle;

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
