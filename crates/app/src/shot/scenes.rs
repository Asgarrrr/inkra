//! The catalogue: one entry per thing that can visibly break.
//!
//! A scene is Rust rather than a line of a script language, so it assembles a
//! `MemFs`, a `Launch` and a gesture with the same vocabulary the test helpers
//! use. A scene that needs something new grows `Stage`, which keeps the
//! runner's reach readable in one place.
//!
//! **No scene may reach a `Window::prompt`.** `Stage` runs the real macOS
//! platform (`stage.rs`), where a prompt is an `NSAlert` shown with
//! `beginSheetModalForWindow` and answered only by a person clicking it;
//! `simulate_prompt_answer` exists on `TestPlatform` alone. A scene that drove
//! the delete confirmation would park for ever, and `capture` would freeze on
//! the frame before it. That is why nothing here deletes a note.

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

/// A paragraph long enough to wrap several times at the column, then a URL,
/// which has no space for a word wrap and breaks mid-word.
const WRAPPED: &str = "Soft wrap keeps a paragraph inside the text column rather than letting \
                       it run off the right edge of the pane. A line longer than seventy \
                       characters paints on several rows, and the caret moves along them.\n\
                       \n\
                       https://example.com/a/very/long/path/that/has/no/spaces/in/it/at/all/so/the/wrap/has/to/break/it/in/the/middle/of/a/word\n\
                       A short last line.";

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

fn sidebar_selected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    // Row 1 is the first note, `projects` being collapsed above it.
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    window
}

/// The field mid-gesture. The one state where a row is not a label, so the
/// thing to look at is that its box matches every other row's — a field even a
/// pixel taller moves every row below it.
fn sidebar_naming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor; see `Stage::focus_tree`.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "d r a f t");
    window
}

/// The same, refused. `index` is already taken, so confirming reports it and
/// leaves the field open rather than closing and doing nothing.
fn sidebar_rejected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor; see `Stage::focus_tree`.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "i n d e x enter");
    window
}

/// The field over a row rather than beside one, holding the name it starts
/// from. The thing to look at is that no row was added: the tree below has not
/// moved, and `index` appears once rather than twice.
fn sidebar_renaming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(VAULT);
    stage.click(window, point(SIDEBAR_X, row_center(1.)));
    // The click handed focus to the editor; see `Stage::focus_tree`.
    stage.focus_tree(window);
    stage.keys(window, "enter");
    window
}

/// Well inside the sidebar's 220 px, clear of the disclosure triangle.
const SIDEBAR_X: gpui::Pixels = px(80.);
