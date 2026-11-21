use gpui::WindowHandle;

use crate::shell::AppShell;
use crate::shot::stage::Stage;

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

pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(HEADINGS);
    stage.focus(window);
    stage.keys(window, "cmd-down");
    window
}

pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(HEADINGS);
    stage.focus(window);
    let keys = format!("cmd-up {}", "right ".repeat(5));
    stage.keys(window, keys.trim_end());
    window
}
