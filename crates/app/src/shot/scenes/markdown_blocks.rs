use gpui::WindowHandle;

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
pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-down home");
    window
}

/// `home` goes to the line's start, on its `>`.
pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-up down home");
    window
}
