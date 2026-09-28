use std::time::Duration;

use gpui::{WindowHandle, point};

use super::{SAMPLE, SIDEBAR_X};
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

pub(super) fn banner(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n"), ("note.md", SAMPLE)]);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    stage.focus(window);
    stage.remove("note.md");
    // The watch's batch latency, on gpui's clock.
    stage.wait(Duration::from_millis(100));
    window
}
