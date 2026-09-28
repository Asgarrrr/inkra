use std::fmt::Write as _;
use std::time::Duration;

use gpui::{WindowHandle, point};

use super::SIDEBAR_X;
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

fn numbered(prefix: &str, count: usize) -> String {
    (0..count).fold(String::new(), |mut text, n| {
        // Writing to a `String` cannot fail.
        let _ = writeln!(text, "{prefix} {n}");
        text
    })
}

/// Up from the end until the caret's top margin puts `line 120` at the viewport's top.
pub(super) fn scroll(stage: &mut Stage) -> WindowHandle<AppShell> {
    let old = numbered("line", 200);
    let window = stage.open_vault(&[("note.md", &old)]);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    stage.focus(window);
    let keys = format!("cmd-down{}", " up".repeat(77));
    stage.keys(window, &keys);
    stage.write("note.md", &format!("{}{old}", numbered("new line", 30)));
    // The watch's batch latency, on gpui's clock.
    stage.wait(Duration::from_millis(100));
    window
}
