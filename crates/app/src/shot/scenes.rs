//! No scene may reach a `Window::prompt`: only a person can answer one, so it would park forever.

mod command_palette;
mod failures;
mod gone;
mod hover;
mod markdown_blocks;
mod quick_open;
mod reload;
mod tabs;
mod vault_search;

use gpui::WindowHandle;

use super::stage::Stage;
use crate::shell::AppShell;

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
        shows: "the tree 4 px under the title row, with no header, ruled against the note on its right: 27 px rounded rows inset 8 px, directories first with a chevron (down when open), indents, a nested note; every name at one depth starts at one x",
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
        shows: "the dirty dot on a note edited but not written, at its row's right edge",
        open: sidebar_dirty,
    },
    Scene {
        name: "sidebar-hover",
        shows: "the pointer on `index`, an unselected row: a 27 px rounded row lit #383737, the same as the selected `projects`, whose name is brighter",
        open: hover::sidebar,
    },
    Scene {
        name: "sidebar-hover-selected",
        shows: "the pointer on the selected `index`: the 27 px rounded row keeps its #383737, no hover colour over it",
        open: hover::sidebar_selected,
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
        name: "save-failed",
        shows: "the note showing could not be written: the failed dot in the failed colour before its tab title and at its tree row's right edge, and one line over the editor naming the note and the reason, a × at its right",
        open: failures::save_failed,
    },
    Scene {
        name: "open-failed",
        shows: "a note that cannot be read: one line over the editor names it and the reason, a × at its right",
        open: failures::open_failed,
    },
    Scene {
        name: "unwatched",
        shows: "a vault the platform refuses to watch: the notice line over the editor from the first frame",
        open: failures::unwatched,
    },
    Scene {
        name: "create-refused",
        shows: "the new-note field kept open with `plan`, the refusal message under it",
        open: failures::create_refused,
    },
    Scene {
        name: "backlinks",
        shows: "the panel right of the editor under its `Backlinks` header, below the title row and ruled on its left against the note: two sources, one linking twice, each link in the link colour",
        open: backlinks,
    },
    Scene {
        name: "tabs",
        shows: "the title row split in two. Over the sidebar: the traffic lights' space, search and new note, the sidebar toggle at the segment's right edge, a hairline down the segment's edge meeting the sidebar column's. Over the note: four tabs as tall as the row, the empty drag space, the backlinks toggle; `index` active, lit from the row's top to its rule and typed into, the dirty dot before its title and its × after; `reading`, `New tab` twice, no × on them; `index` highlighted in the sidebar with its dot at the row's right edge; both toggles lit; the tree 4 px under the row with no header, the `Backlinks` header level under the row, ruled like it",
        open: tabs::dirty,
    },
    Scene {
        name: "panels-hidden",
        shows: "`cmd-\\` and `cmd-|`: no sidebar, no backlinks, the note column the window's width under the title row; no sidebar segment, no search or new-note button; both toggles muted, the sidebar's after the traffic lights' space leading the tabs and the backlinks' at the row's right end",
        open: tabs::panels_hidden,
    },
    Scene {
        name: "toggle-hover",
        shows: "the pointer on the sidebar toggle at the sidebar segment's right edge: the 28 x 26 button hover behind its lit icon, mid-row; search and new note unhovered to its left",
        open: tabs::toggle_hover,
    },
    Scene {
        name: "tabs-hover",
        shows: "the pointer on the inactive `reading`: the row hover behind it and its × shown; the other inactive tabs show no ×",
        open: tabs::hover,
    },
    Scene {
        name: "tabs-dragging",
        shows: "`index` dragged onto the third tab, button held: that tab lit in the drop colour with a 2 px right edge, the preview under the pointer; the bar's order unchanged until the drop",
        open: tabs::dragging,
    },
    Scene {
        name: "tabs-overflow",
        shows: "30 tabs over the note, the last one active (`cmd-9`) and scrolled into view; the empty drag space after the bar kept at its 32 px floor before the backlinks toggle; the sidebar segment unchanged: the lights, search and new note, the toggle at its right edge, its hairline",
        open: tabs::overflow,
    },
    Scene {
        name: "tabs-restored",
        shows: "a relaunch: `index`, `reading` and `today` back in the bar, `reading` active with `Books to read,` selected; `reading` highlighted in the sidebar; no \"Select a note.\"",
        open: tabs::restored,
    },
    Scene {
        name: "quick-open-results",
        shows: "cmd-p over the editor: name rows then text rows, word starts lit, a dimmed second line under each, folders cut from the start",
        open: quick_open::results,
    },
    Scene {
        name: "picker-hover",
        shows: "quick open's results with the pointer on row 2: the hover background there, row 0 still selected",
        open: hover::picker,
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
    Scene {
        name: "vault-search-results",
        shows: "cmd-shift-f `plan`: a header per note (name, folder dimmed, count; `100+` past the cap) under a hairline, then its hits, line numbers 998 to 1001 right-aligned in one column, the word lit",
        open: vault_search::results,
    },
    Scene {
        name: "vault-search-empty",
        shows: "cmd-shift-f with nothing typed: the placeholder \"Search notes…\" and no rows",
        open: vault_search::empty,
    },
    Scene {
        name: "vault-search-no-match",
        shows: "a query nothing matches: \"No matches\"",
        open: vault_search::no_match,
    },
    Scene {
        name: "vault-search-indexing",
        shows: "a load still pending: the loaded notes' hits, \"Indexing 2 of 3 notes\" dimmed under them",
        open: vault_search::indexing,
    },
    Scene {
        name: "vault-search-limit",
        shows: "1,100 hits: the first 1,000 listed, \"1,000+ results; refine the query\" dimmed under them",
        open: vault_search::limit,
    },
    Scene {
        name: "vault-search-scanning",
        shows: "cmd-shift-f before the vault's first scan ends: \"Scanning the vault…\"",
        open: vault_search::scanning,
    },
    Scene {
        name: "vault-search-opened",
        shows: "enter on `step`, first found on line 998 of `roadmap.md`: the line centred, `step` selected",
        open: vault_search::opened,
    },
    Scene {
        name: "command-palette",
        shows: "cmd-shift-p over the editor, n typed: every command with a word starting n, lit, its key right and dimmed",
        open: command_palette::open,
    },
    Scene {
        name: "markdown-blocks",
        shows: "quotes: bars per level, every row at the gutter's edge, a lazy line, a heading and a list in a quote",
        open: markdown_blocks::concealed,
    },
    Scene {
        name: "markdown-blocks-revealed",
        shows: "the caret on a quote's first line: its > paints in the gutter, no text moved",
        open: markdown_blocks::revealed,
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
    stage.click_tree_row(window, 0.);
    window
}

fn sidebar_dirty(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = sidebar_selected(stage);
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
    stage.click_tree_row(window, 1.);
    window
}

fn sidebar_naming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = sidebar_selected(stage);
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "d r a f t");
    window
}

fn sidebar_rejected(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = sidebar_selected(stage);
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "cmd-n");
    stage.keys(window, "i n d e x enter");
    window
}

fn sidebar_renaming(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = sidebar_selected(stage);
    // The click handed focus to the editor.
    stage.focus_tree(window);
    stage.keys(window, "enter");
    window
}

fn backlinks(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(LINKED);
    stage.click_tree_row(window, 0.);
    window
}
