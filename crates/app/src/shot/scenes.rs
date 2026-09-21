//! The catalogue: one entry per thing that can visibly break.
//!
//! A scene is Rust rather than a line of a script language, so it assembles a
//! `MemFs`, a `Launch` and a gesture with the same vocabulary the test helpers
//! use. A scene that needs something new grows `Stage`, which keeps the
//! runner's reach readable in one place.

use gpui::{WindowHandle, point, px};

use super::stage::Stage;
use crate::shell::AppShell;
use crate::sidebar::row_center;

/// A named state of the window, and the PNG it writes.
pub(crate) struct Scene {
    /// The name on the command line, and the file's stem.
    pub(crate) name: &'static str,
    /// What the shot is for. Printed by `--list`, so the catalogue explains
    /// itself without opening this file.
    pub(crate) shows: &'static str,
    pub(crate) open: fn(&mut Stage) -> WindowHandle<AppShell>,
}

/// Enough lines to exercise per-line layout, a blank line, and a line long
/// enough to reach for the right-hand edge.
const SAMPLE: &str = "The quick brown fox jumps over the lazy dog.\n\
                      Sphinx of black quartz, judge my vow.\n\
                      \n\
                      A third paragraph, after a blank line.";

/// Nested so the sidebar's indent and its directories-first order both show.
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
            name: "sidebar-vault",
            shows: "the tree: directories first, indents, a nested note",
            open: sidebar_vault,
        },
        Scene {
            name: "sidebar-dirty",
            shows: "the dirty dot on a note edited but not written",
            open: sidebar_dirty,
        },
    ]
}

/// The launch state with nothing to edit. Focused, because the focus lands on
/// `AppShell`'s own handle here rather than an editor's, and a shot is the only
/// way to see that it lands somewhere harmless.
///
/// The vault-with-no-note variant of the empty state needs no scene of its own:
/// `sidebar-vault` below already opens a vault and clicks a directory, not a
/// note, so it draws that state beside the tree.
fn empty_no_vault(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_nothing();
    stage.focus(window);
    window
}

/// Unfocused on purpose: this is the one scene that shows the text alone, with
/// no caret drawn over it.
fn editor_text(stage: &mut Stage) -> WindowHandle<AppShell> {
    stage.open_note(SAMPLE)
}

fn editor_caret(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(SAMPLE);
    stage.focus(window);
    // From the document's start rather than wherever the buffer left the
    // caret, so the shot does not move when `EditorView::new`'s initial
    // selection does.
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

fn sidebar_vault(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    // Row 0 is the `projects` directory; opening it is what makes the indent
    // and the nested `archive` visible at all.
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    window
}

fn sidebar_dirty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    // With `projects` collapsed, the notes follow it: row 1 is the first.
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    stage.focus(window);
    stage.keys(window, "e d i t e d");
    window
}

/// Well inside the sidebar's 220 px, clear of the disclosure triangle.
const SIDEBAR_X: gpui::Pixels = px(80.);
