use gpui::WindowHandle;

use super::Scene;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// `note` with `keys` pressed in it, focused.
fn typed(stage: &mut Stage, note: &str, keys: &str) -> WindowHandle<AppShell> {
    let window = stage.open_note(note);
    stage.focus(window);
    stage.keys(window, keys);
    window
}

fn ordered(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "1. a", "cmd-down enter")
}

fn task(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "- [ ] a", "cmd-down enter")
}

fn quoted(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "> - a", "cmd-down enter")
}

fn leaves(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "- a\n- ", "cmd-down enter")
}

fn wider(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "9. a\n   - c", "cmd-up end enter")
}

pub(super) const SCENES: [Scene; 5] = [
    Scene {
        name: "list-enter-ordered",
        shows: "`1. a` then enter: a `2.` on the next row, its number in the slot `1.` uses, the caret at its text start",
        open: ordered,
    },
    Scene {
        name: "list-enter-task",
        shows: "`- [ ] a` then enter: a second, open box under the first, the caret right of it",
        open: task,
    },
    Scene {
        name: "list-enter-quoted",
        shows: "`> - a` then enter: the new `•` inside the quote, its bar running down both rows, the caret at its text start",
        open: quoted,
    },
    Scene {
        name: "list-enter-leaves",
        shows: "`- a` and an empty item, then enter: the empty item's marker gone, one `•` left, the caret on an empty plain line under it",
        open: leaves,
    },
    Scene {
        name: "list-enter-wider",
        shows: "`9. a` over a child `- c`, then enter: an empty `10.` between them, `9.` and `10.` on one number column, the child's `•` still under the new item's text",
        open: wider,
    },
];
