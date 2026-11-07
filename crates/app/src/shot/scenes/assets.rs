use std::time::Duration;

use gpui::WindowHandle;

use crate::images::png;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// Assets beside notes and in a folder, and a save's leftover temp file the tree hides.
const FILES: &[(&str, &str)] = &[
    ("index.md", "# Index\n\nThe vault's front page.\n"),
    ("reading.md", "Books to read.\n"),
    ("photo.png", "PNG"),
    ("report.pdf", "%PDF"),
    ("index.md.inkra-1-2.tmp", "half a save"),
    ("projects/inkra.md", "# Inkra\n"),
    ("projects/diagram.svg", "<svg/>"),
];

pub(super) fn tree(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(FILES);
    stage.click_tree_row(window, 0.);
    window
}

pub(super) fn image(stage: &mut Stage) -> WindowHandle<AppShell> {
    photo(stage, png(640, 400))
}

pub(super) fn image_failed(stage: &mut Stage) -> WindowHandle<AppShell> {
    photo(stage, b"not an image".to_vec())
}

/// `photo.png`, holding `bytes`, clicked in the tree beside a note that embeds it.
fn photo(stage: &mut Stage, bytes: Vec<u8>) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("a.md", "![[photo.png]]\n")]);
    stage.write_bytes("photo.png", bytes);
    // The watch's batch latency, on gpui's clock.
    stage.wait(Duration::from_millis(100));
    stage.click_tree_row(window, 1.);
    window
}
