use gpui::WindowHandle;

use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// Checked against pulldown-cmark 0.13.4: the lazy line joins the nested quote, the heading and the list the outer one.
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
                      Body text after the quotes.";

pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-down");
    window
}

/// `home` goes to the line's start, on its `>`.
pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(QUOTES);
    stage.focus(window);
    stage.keys(window, "cmd-up down home");
    window
}
