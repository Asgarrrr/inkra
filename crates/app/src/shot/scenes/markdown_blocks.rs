use gpui::WindowHandle;

use super::Scene;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// Checked against pulldown-cmark 0.13.4: the lazy line joins the nested quote, the heading and the list the outer one.
/// Every task gets a box; an outer item's marker paints before it (task boxes spec, D10).
const QUOTES: &str = "A paragraph before the quotes.\n\
                      > A quote over two lines,\n\
                      > the second one here.\n\
                      >\n\
                      > A quote line long enough to wrap: its second row starts at the gutter's edge, clear of the bar.\n\
                      > > A nested quote, two bars,\n\
                      > > that runs on\n\
                      with a lazy line.\n\
                      > # A heading in a quote\n\
                      > - a list in a quote\n\
                      \n\
                      Body text after the quotes.\n\
                      \n\
                      - [ ] An open task\n\
                      \x20\x20- [ ] A task under the open task\n\
                      \x20\x20- A bullet under the open task\n\
                      - A plain bullet beside the tasks\n\
                      - [x] A done task\n\
                      \x20\x20- [ ] A nested task\n\
                      \t- [ ] A task nested by a tab\n\
                      1. [ ] An ordered task\n\
                      - - [ ] A task after a bullet\n\
                      1. - [x] A done task after a number\n\
                      \n\
                      > - [ ] A quoted task\n\
                      with a lazy line at the quote's edge.\n\
                      \n\
                      - [x] A done task long enough to wrap: its second row starts at the gutter's edge, under its text and not under the hidden marker.\n\
                      \n\
                      2. [ ] A revealed ordered task";

/// `home` puts the caret on the last task's number.
fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-down home");
    window
}

/// `home` goes to the line's start, on its `>`.
fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-up down home");
    window
}

pub(super) const SCENES: [Scene; 2] = [
    Scene {
        name: "markdown-blocks",
        shows: "quotes: bars per level, every row at the gutter's edge, a lazy line, a heading and a list in a quote; tasks open, done, nested by spaces and by a tab, ordered, after a bullet, after a number, quoted with a lazy line at the quote's edge, and wrapped, beside a plain bullet: each task's box where the bullet's `•` is, on its first row, and its text in the bullet's text column; under the open task a nested task and a nested bullet, their box and `•` under the open task's text; the box right after its lead: an ordered task's number, the `•` of a task after a bullet; open a grey outline, done an accent fill with a check and its text struck and dimmed on every row, the nested open task under it unstruck, none on the lazy line; the last ordered task, the caret on its number, reads raw in a widened gutter, its text right of the box column",
        open: concealed,
    },
    Scene {
        name: "markdown-blocks-revealed",
        shows: "the caret on a quote's first line: its > paints in the gutter, no text moved",
        open: revealed,
    },
];
