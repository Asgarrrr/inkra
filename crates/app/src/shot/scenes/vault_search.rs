use gpui::WindowHandle;

use super::UNSCANNED;
use super::quick_open::PICKER_SETTLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// Two short notes, then one past the per-note cap whose hits start at line 998, so the
/// column holds three digits, then four.
fn notes() -> Vec<(String, String)> {
    let long = (1..=1_130)
        .map(|week| {
            if week < 998 {
                format!("Week {week}: rest\n")
            } else {
                format!("Week {week}: plan the next step\n")
            }
        })
        .collect::<String>();
    vec![
        (
            "journal/2026-09-27.md".into(),
            "Met Ana about the release plan.\n\n\tShe will plan the demo, then the long tail of work that follows it, which runs past the right edge.\n".into(),
        ),
        (
            "projects/inkra/plan.md".into(),
            "# Plan\n\n- Ship smart quick open, then `vault search`.\n".into(),
        ),
        ("roadmap.md".into(), long),
    ]
}

fn open(stage: &mut Stage, files: &[(String, String)]) -> WindowHandle<AppShell> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let window = stage.open_vault(&files);
    stage.focus(window);
    stage.keys(window, "cmd-shift-f");
    window
}

pub(super) fn results(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = open(stage, &notes());
    stage.keys(window, "p l a n");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn empty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = open(stage, &notes());
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn no_match(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = open(stage, &notes());
    stage.keys(window, "z q x");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn indexing(stage: &mut Stage) -> WindowHandle<AppShell> {
    let files = notes();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let window = stage.open_vault_loading(&files, "roadmap.md");
    stage.focus(window);
    stage.keys(window, "cmd-shift-f");
    stage.keys(window, "p l a n");
    stage.wait(PICKER_SETTLE);
    window
}

/// Eleven notes of 100 hits: the first 1,000 list.
pub(super) fn limit(stage: &mut Stage) -> WindowHandle<AppShell> {
    let files: Vec<(String, String)> = (1..=11)
        .map(|n| {
            (
                format!("plans/plan {n:02}.md"),
                "A step of the plan\n".repeat(100),
            )
        })
        .collect();
    let window = open(stage, &files);
    stage.keys(window, "p l a n");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn scanning(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_unscanned_vault(UNSCANNED, "projects");
    stage.focus(window);
    stage.keys(window, "cmd-shift-f");
    stage.keys(window, "i n d e x");
    stage.wait(PICKER_SETTLE);
    window
}
