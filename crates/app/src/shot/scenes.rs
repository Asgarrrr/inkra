//! No scene may reach a `Window::prompt`: only a person can answer one, so it would park forever.

use gpui::{WindowHandle, point, px};

use super::stage::Stage;
use crate::shell::AppShell;
use crate::sidebar::row_center;

pub(crate) struct Scene {
    pub(crate) name: &'static str,
    pub(crate) shows: &'static str,
    pub(crate) open: fn(&mut Stage) -> WindowHandle<AppShell>,
}

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog.\n\
                      Sphinx of black quartz, judge my vow.\n\
                      \n\
                      A third paragraph, after a blank line.";

const WRAPPED: &str = "Soft wrap keeps a paragraph inside the text column rather than letting \
                       it run off the right edge of the pane. A line longer than seventy \
                       characters paints on several rows, and the caret moves along them.\n\
                       \n\
                       https://example.com/a/very/long/path/that/has/no/spaces/in/it/at/all/so/the/wrap/has/to/break/it/in/the/middle/of/a/word\n\
                       A short last line.";

const VAULT: &[(&str, &str)] = &[
    ("index.md", "# Index\n\nThe vault's front page.\n"),
    ("reading.md", "Books to read.\n"),
    ("projects/inkra.md", "# Inkra\n\nA note editor.\n"),
    ("projects/archive/old.md", "Superseded.\n"),
];

pub(crate) fn catalogue() -> &'static [Scene] {
    &[
        Scene {
            name: "empty-no-vault",
            shows: "launch with no argument: the empty state, and no editor at all",
            open: empty_no_vault,
        },
        Scene {
            name: "editor-text",
            shows: "per-line layout, a blank line, real font metrics",
            open: editor_text,
        },
        Scene {
            name: "editor-caret",
            shows: "the caret, mid-document, in a focused window",
            open: editor_caret,
        },
        Scene {
            name: "editor-selection",
            shows: "a selection over four lines: both ends, and a blank line's stub",
            open: editor_selection,
        },
        Scene {
            name: "editor-wrapped",
            shows: "soft wrap: a paragraph and a URL in the centred column, caret on a wrapped row",
            open: editor_wrapped,
        },
        Scene {
            name: "editor-wrapped-selection",
            shows: "a selection across a wrap and a newline: one quad per row, a stub only at the newline",
            open: editor_wrapped_selection,
        },
        Scene {
            name: "sidebar-vault",
            shows: "the tree: directories first, indents, a nested note",
            open: sidebar_vault,
        },
        Scene {
            name: "sidebar-selected",
            shows: "the selected row, which is what a tree command acts on",
            open: sidebar_selected,
        },
        Scene {
            name: "sidebar-naming",
            shows: "a name being typed into the tree: the field's row, caret and indent",
            open: sidebar_naming,
        },
        Scene {
            name: "sidebar-rejected",
            shows: "a refused name, with the reason under the field",
            open: sidebar_rejected,
        },
        Scene {
            name: "sidebar-renaming",
            shows: "a rename in progress: the field replaces the row, holding the old name",
            open: sidebar_renaming,
        },
        Scene {
            name: "sidebar-dirty",
            shows: "the dirty dot on a note edited but not written",
            open: sidebar_dirty,
        },
    ]
}

fn empty_no_vault(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_nothing();
    stage.focus(window);
    window
}

/// Unfocused on purpose: the one scene with no caret drawn.
fn editor_text(stage: &mut Stage) -> WindowHandle<AppShell> {
    stage.open_note(SAMPLE)
}

fn editor_caret(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(SAMPLE);
    stage.focus(window);
    stage.keys(window, "cmd-up down right right right right");
    window
}

fn editor_selection(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(SAMPLE);
    stage.focus(window);
    stage.keys(
        window,
        "cmd-up right right right right shift-down shift-down shift-right shift-right",
    );
    window
}

fn editor_wrapped(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(WRAPPED);
    stage.focus(window);
    stage.keys(window, "cmd-up down right right right");
    window
}

fn editor_wrapped_selection(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(WRAPPED);
    stage.focus(window);
    stage.keys(
        window,
        "cmd-up down right right right shift-down shift-down shift-right",
    );
    window
}

fn sidebar_vault(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    window
}

fn sidebar_dirty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    stage.focus(window);
    stage.keys(window, "e d i t e d");
    window
}

fn sidebar_selected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    window
}

fn sidebar_naming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "d r a f t");
    window
}

fn sidebar_rejected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "i n d e x enter");
    window
}

fn sidebar_renaming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "enter");
    window
}

const SIDEBAR_X: gpui::Pixels = px(80.);
