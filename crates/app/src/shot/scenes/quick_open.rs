use std::time::Duration;

use gpui::{WindowHandle, point};

use super::{SIDEBAR_X, UNSCANNED};
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

/// Texts give each row its second line: a name row its first prose line, a text row the match.
const NOTES: &[(&str, &str)] = &[
    (
        "index.md",
        "# Index\n\nThe front page. Read the release plan before Friday.\n",
    ),
    (
        "reading list.md",
        "Books to read while planning the next trip.\n",
    ),
    (
        "projects/inkra/plan.md",
        "# Plan\n\n- **Ship** smart quick open, then `vault search`.\n",
    ),
    (
        "projects/inkra/release plan.md",
        "Release on Friday, after the last review.\n",
    ),
    (
        "projects/inkra/a plan whose name alone is wider than the whole row of the picker, and then some more words after that.md",
        "A first line long enough to run past the right edge of the picker, so it ends in an ellipsis.\n",
    ),
    (
        "archive/2025/old projects/a very long folder name that runs past the row/and keeps going for a while longer still/projects/plan b.md",
        "Superseded by the plan above.\n",
    ),
];

/// With `NOTES`, `plan friday` lists one row per group.
const FRIDAY_PLAN: &[(&str, &str)] = &[("friday plan.md", "What ships before the weekend.\n")];

const GRAPHEMES: &[(&str, &str)] = &[
    (
        "cafe\u{301} notes.md",
        "Un e\u{301}te\u{301} a\u{300} Lyon, cafe\u{301} au lait.\n",
    ),
    (
        "\u{1f469}\u{200d}\u{1f4bb} dev log.md",
        "\u{1f469}\u{200d}\u{1f4bb}editor fixes, every day.\n",
    ),
    (
        "sprint e\u{301}te\u{301}/\u{1f469}\u{200d}\u{1f4bb} review.md",
        "Retro notes.\n",
    ),
];

/// Longer than any picker wait, so the last query has answered.
pub(super) const PICKER_SETTLE: Duration = Duration::from_millis(300);

pub(super) fn results(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(NOTES);
    // Folders come first, so `index` is row 2.
    stage.click(window, point(SIDEBAR_X, row_center(2.)));
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.keys(window, "p l a n");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn groups(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[NOTES, FRIDAY_PLAN].concat());
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.keys(window, "p l a n space f r i d a y");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn text_only(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(NOTES);
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.keys(window, "f r i d a y");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn indexing(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault_loading(NOTES, "projects/inkra/plan.md");
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.keys(window, "p l a n");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn empty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(NOTES);
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn no_match(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(NOTES);
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.keys(window, "z q x");
    stage.wait(PICKER_SETTLE);
    window
}

/// `e` lights `é` whole: the index holds the name as NFC, the text as written.
pub(super) fn graphemes(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(GRAPHEMES);
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.type_text(window, "e");
    stage.wait(PICKER_SETTLE);
    window
}

pub(super) fn scanning(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_unscanned_vault(UNSCANNED, "projects");
    stage.focus(window);
    stage.keys(window, "cmd-p");
    stage.wait(PICKER_SETTLE);
    window
}
