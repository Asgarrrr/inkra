use std::time::Duration;

use gpui::WindowHandle;

use super::SAMPLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

pub(super) fn banner(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)]);
    stage.click_tree_row(window, 1.);
    stage.focus(window);
    stage.remove("note.md");
    // The watch's batch latency, on gpui's clock.
    stage.wait(Duration::from_millis(100));
    window
}
