//! No scene may reach a `Window::prompt`: only a person can answer one, so it would park forever.

mod gone;
mod quick_open;
mod reload;

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

const MARKDOWN: &str = "---\n\
                        title: Styled source\n\
                        ---\n\
                        # Heading one\n\
                        ## Heading two ##\n\
                        \n\
                        Plain, **strong**, *emphasis*, ***both***, ~~struck~~ and `code`.\n\
                        \n\
                        A [link](https://example.com), an <https://auto.link>, a [[Wiki Page|wikilink]], and ![an image](a.png).\n\
                        > A quote that runs onto\n\
                        > a second line.\n\
                        - a bullet with **bold**\n\
                        - [ ] a task\n\
                        1. a numbered item\n\
                        ```rust\n\
                        let x = 1;\n\
                        ```\n\
                        ---\n\
                        | a | b |\n\
                        |---|---|\n\
                        | 1 | 2 |";

const HEADINGS: &str = "# Heading one, long enough that it wraps onto a second row at this size\n\
                        Body text under it.\n\
                        ## Heading two\n\
                        ### Heading three\n\
                        #### Heading four\n\
                        ##### Heading five\n\
                        ###### Heading six\n\
                        Setext heading\n\
                        ==============\n\
                        Last body line.";

const VAULT: &[(&str, &str)] = &[
    ("index.md", "# Index\n\nThe vault's front page.\n"),
    ("reading.md", "Books to read.\n"),
    ("projects/inkra.md", "# Inkra\n\nA note editor.\n"),
    ("projects/archive/old.md", "Superseded.\n"),
];

/// No folders, so `index` is the first row.
const LINKED: &[(&str, &str)] = &[
    ("index.md", "# Index\n\nThe vault's front page.\n"),
    ("reading.md", "Books to read, listed from the [[index]].\n"),
    (
        "today.md",
        "Back to [[index]] after lunch.\nThen [[index|the front page]] once more.\n",
    ),
];

/// Not a function body: it grows with every scene, past clippy's `too_many_lines`.
const CATALOGUE: &[Scene] = &[
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
        name: "editor-markdown",
        shows: "every construct styled, syntax concealed except a closing ## (dimmed): offset 0 reveals nothing",
        open: editor_markdown,
    },
    Scene {
        name: "editor-concealed",
        shows: "concealment: caret at the end, markers hidden except a closing ## (dimmed), bullets as dots",
        open: editor_concealed,
    },
    Scene {
        name: "editor-revealed-bold",
        shows: "the caret inside **strong** reveals that element only",
        open: editor_revealed_bold,
    },
    Scene {
        name: "editor-revealed-link",
        shows: "the caret inside the wikilink reveals its [[target| and ]] only",
        open: editor_revealed_link,
    },
    Scene {
        name: "editor-headings",
        shows: "heading sizes H1 to H6 and setext, caret on body text so every `#` is hidden",
        open: editor_headings,
    },
    Scene {
        name: "editor-heading-revealed",
        shows: "the caret on the wrapped H1: `# ` shows dimmed in the margin, and no text moves",
        open: editor_heading_revealed,
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
    Scene {
        name: "sidebar-root-only",
        shows: "the first frames of a launch: the root level, folders closed, while the rest is scanned",
        open: sidebar_root_only,
    },
    Scene {
        name: "reload-scroll",
        shows: "30 lines inserted on disk above the viewport: `line 120` stays at the top, the caret on `line 123`",
        open: reload::scroll,
    },
    Scene {
        name: "gone-from-disk",
        shows: "the note showing deleted on disk: one line over the editor says so, its text kept, gone from the tree",
        open: gone::banner,
    },
    Scene {
        name: "backlinks",
        shows: "the panel right of the editor: two sources, one linking twice, each link in the link colour",
        open: backlinks,
    },
    Scene {
        name: "quick-open-results",
        shows: "cmd-p over the editor: name rows then text rows, word starts lit, a dimmed second line under each, folders cut from the start",
        open: quick_open::results,
    },
    Scene {
        name: "quick-open-groups",
        shows: "`plan friday`: one row per group, name then name and text then text, each with its second line",
        open: quick_open::groups,
    },
    Scene {
        name: "quick-open-text-only",
        shows: "`friday`, in no name: text rows only, the word lit in each excerpt",
        open: quick_open::text_only,
    },
    Scene {
        name: "quick-open-indexing",
        shows: "a load still pending: \"Indexing 5 of 6 notes\" dimmed under the rows, the unloaded note with no second line",
        open: quick_open::indexing,
    },
    Scene {
        name: "quick-open-empty",
        shows: "cmd-p before any note was shown: \"No recent notes\" under the placeholder",
        open: quick_open::empty,
    },
    Scene {
        name: "quick-open-no-match",
        shows: "a query nothing matches: \"No matching notes\"",
        open: quick_open::no_match,
    },
    Scene {
        name: "quick-open-graphemes",
        shows: "`e` lights a decomposed `é` whole, in a folder and in an excerpt, and a word right after an emoji ZWJ sequence: no garbled glyph",
        open: quick_open::graphemes,
    },
    Scene {
        name: "quick-open-scanning",
        shows: "cmd-p before the vault's first scan ends: \"Scanning the vault…\" under the placeholder",
        open: quick_open::scanning,
    },
];

pub(crate) fn catalogue() -> &'static [Scene] {
    CATALOGUE
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

fn editor_markdown(stage: &mut Stage) -> WindowHandle<AppShell> {
    stage.open_note(MARKDOWN)
}

fn editor_concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(MARKDOWN);
    stage.focus(window);
    stage.keys(window, "cmd-down");
    window
}

fn editor_revealed_bold(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(MARKDOWN);
    stage.focus(window);
    let keys = format!("cmd-up {}{}", "down ".repeat(6), "right ".repeat(10));
    stage.keys(window, keys.trim_end());
    window
}

/// `ctrl-e` goes to the logical line end, so the wrap of the link line cannot shift the landing.
fn editor_revealed_link(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(MARKDOWN);
    stage.focus(window);
    let keys = format!("cmd-up {}ctrl-e {}", "down ".repeat(8), "left ".repeat(31));
    stage.keys(window, keys.trim_end());
    window
}

fn editor_headings(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(HEADINGS);
    stage.focus(window);
    stage.keys(window, "cmd-down");
    window
}

fn editor_heading_revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(HEADINGS);
    stage.focus(window);
    let keys = format!("cmd-up {}", "right ".repeat(5));
    stage.keys(window, keys.trim_end());
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

/// Three root notes and two folders; the walk is held at `projects`.
const UNSCANNED: &[(&str, &str)] = &[
    ("index.md", "# Index\n"),
    ("reading.md", "Books to read.\n"),
    ("todo.md", "Things to do.\n"),
    ("projects/inkra.md", "# Inkra\n"),
    ("archive/old.md", "Superseded.\n"),
];

fn sidebar_root_only(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_unscanned_vault(UNSCANNED, "projects");
    stage.focus(window);
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

fn backlinks(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    window
}
