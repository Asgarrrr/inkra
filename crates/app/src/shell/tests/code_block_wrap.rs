//! `settings.json` sets and clears `code_block_wrap` live (code block scroll spec, D7).

use std::sync::Arc;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;

use super::themes::{open_theme_picker, theme_picker};
use super::{AppShell, editor, open, shell_focused};
use crate::test_support;
use crate::theme;

const SETTINGS: &str = "/vault/.inkra/settings.json";

/// One block line too wide for the column: four rows scrolled, five wrapped.
fn open_block(cx: &mut TestAppContext) -> (Arc<MemFs>, WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(
        "/vault/a.md",
        format!("```\n{}\n```\nbody", "x".repeat(120)),
    );
    fs.insert_file(SETTINGS, "{}");
    let (window, notes, mut visual) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut visual, "a");
    open(&window, &mut visual, a);
    (fs, window, visual)
}

fn rows(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> usize {
    editor(window, cx).update(cx, |editor, _| editor.row_count())
}

/// Sets the key as a person edits the file, then waits for the watcher's reload.
fn write(fs: &MemFs, settings: &str, cx: &mut VisualTestContext) {
    fs.insert_file(SETTINGS, settings);
    cx.run_until_parked();
}

fn set_and_cleared(fs: &MemFs, window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) {
    assert_eq!(rows(window, cx), 4);
    let theme = cx.update(|_, cx| theme::current(cx).clone());

    write(fs, r#"{"code_block_wrap":true}"#, cx);
    assert_eq!(rows(window, cx), 5);
    assert!(Arc::ptr_eq(
        &cx.update(|_, cx| theme::current(cx).clone()),
        &theme
    ));

    write(fs, "{}", cx);
    assert_eq!(rows(window, cx), 4);
    assert!(Arc::ptr_eq(
        &cx.update(|_, cx| theme::current(cx).clone()),
        &theme
    ));
}

#[gpui::test]
fn the_wrap_setting_applies_live(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = open_block(cx);
    set_and_cleared(&fs, &window, &mut visual);
}

#[gpui::test]
fn the_wrap_setting_applies_with_the_picker_open(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = open_block(cx);
    open_theme_picker(&mut visual);
    assert!(theme_picker(window, &mut visual).is_some());

    set_and_cleared(&fs, &window, &mut visual);

    assert!(theme_picker(window, &mut visual).is_some(), "still open");
}
