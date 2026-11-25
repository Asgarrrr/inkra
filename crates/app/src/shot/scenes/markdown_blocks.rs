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
                      with a lazy line under its text.\n\
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

/// A list's marker lines and its other lines: continuation, lazy and code lines start at their
/// item's text; one list's numbers share a width; an empty item's mark sits as a typed one's (task
/// boxes spec, D3, D4, D11).
const LISTS: &str = "8. Eight\n\
                     9. Nine\n\
                     \x20\x20\x20and its second line\n\
                     10. Ten\n\
                     \x20\x20\x20\x20- A child under ten\n\
                     11. Eleven\n\
                     \n\
                     - A top bullet\n\
                     \x20\x20- Nested by two spaces\n\
                     \x20\x20\x20\x20- Three levels deep\n\
                     \x20\x20\x20\x20\x20\x20with its second line\n\
                     - A bullet with a child by four spaces\n\
                     \x20\x20\x20\x20- Four spaces in\n\
                     - A bullet with a child by a tab\n\
                     \t- A tab in\n\
                     - A bullet long enough to wrap: its second row starts at its text, under the first word and not under the dot.\n\
                     - A bullet with a continuation\n\
                     \x20\x20line, then\n\
                     a lazy line.\n\
                     - [ ] A task beside the bullets\n\
                     - [x] A done task\n\
                     - A bullet after the tasks\n\
                     \n\
                     1) One\n\
                     2) Two\n\
                     3) Three\n\
                     \n\
                     > - A quoted bullet\n\
                     >   with its second line\n\
                     \n\
                     1. [ ] An ordered task\n\
                     \x20\x20\x20- Its child\n\
                     2.\x20\n\
                     \n\
                     - # A heading in a bullet\n\
                     -\x20\n\
                     - [ ] A task holding code:\n\
                     \n\
                     \x20\x20```rust\n\
                     \x20\x20let x = 1;\n\
                     \n\
                     \x20\x20if x > 0 {\n\
                     \x20\x20\x20\x20\x20\x20let y = 2;\n\
                     \x20\x20}\n\
                     \x20\x20```";

/// Eight lines down, the caret lands on the nested bullet's line; `home` puts it before its `  - `.
fn caret_on_a_nested_bullet(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(LISTS);
    stage.focus(window);
    stage.keys(
        window,
        "cmd-up down down down down down down down down home",
    );
    window
}

/// The note's end, past what the first scene's window holds.
fn lists_end(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(LISTS);
    stage.focus(window);
    stage.keys(window, "cmd-down");
    window
}

pub(super) const SCENES: [Scene; 13] = [
    Scene {
        name: "markdown-blocks",
        shows: "quotes: bars per level, every row at the gutter's edge, a lazy line, a heading and a list in a quote; tasks open, done, nested by spaces and by a tab, ordered, after a bullet, after a number, quoted with a lazy line under its text, and wrapped, beside a plain bullet: each task's box in a slot where the bullet's `•` is, on its first row, a slot's gap before its text, and its text in the bullet's text column; under the open task a nested task and a nested bullet, their box and `•` under the open task's text; an ordered task's number in its slot then its box, a task after a bullet its `•` then its box; open a grey outline, done an accent fill with a check and its text struck and dimmed on every row, the nested open task under it unstruck, none on the lazy line; the last ordered task, the caret on its number, reads raw `2. [ ] ` ending at its text, which stays in the ordered task's text column",
        open: concealed,
    },
    Scene {
        name: "markdown-blocks-revealed",
        shows: "the caret on a quote's first line: its > paints in the gutter, no text moved",
        open: revealed,
    },
    Scene {
        name: "markdown-lists",
        shows: "8. to 11. on one number column, their dots aligned across 9. to 10., their texts in one column, 9.'s second line at its text, 10.'s child `•` under its text; list marker lines in slots: each `•` centred on the box's axis, each text one slot right; children by two spaces, four spaces and a tab start their slot at the parent's text, three levels deep; a wrapped bullet's second row at its text; tasks' boxes in the bullets' column, their text in the bullets' text column; a third-level bullet's second line, a continuation line and a lazy line at their item's text; the caret on the second line's start shows its raw `  - ` ending at its text, which does not move",
        open: caret_on_a_nested_bullet,
    },
    Scene {
        name: "markdown-lists-end",
        shows: "the same note's end: 1) to 3) on their own column; a quoted bullet's second line at its text; an ordered task's number then its box, its child under its text, then an empty `2.` on its row as `1.` sits on its own; a `•` on a heading's baseline, then an empty item's `•` where a typed item's sits; a task holding code: the caret on the closing fence shows both fences; the code block's lines, its blank line included, start at the task's text, and so does its background; `let y = 2;` keeps its own indent, four columns right of `if`",
        open: lists_end,
    },
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
    Scene {
        name: "list-tab-nests",
        shows: "`- a` over `- b`, then tab on `b`: its `•` one slot right, under `a`'s text, the caret after `b`",
        open: nests,
    },
    Scene {
        name: "list-tab-ordered",
        shows: "`1. a` over `2. b`, then tab on `b`: a `1.` under `a`'s text, the caret after `b`",
        open: nests_ordered,
    },
    Scene {
        name: "list-shift-tab-adopts",
        shows: "`- a` over `1. b`, `2. c` and `3. d` nested, then shift-tab on `b`: `b`'s `•` under `a`'s, `c` and `d` renumbered `1.` and `2.` under `b`'s text",
        open: adopts,
    },
    Scene {
        name: "list-enter-nested",
        shows: "`- a` over `- b` and an empty nested item, then enter: the empty item's `•` one level up, under `a`'s, the caret right of it",
        open: unnests,
    },
];

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

fn nests(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "- a\n- b", "cmd-down tab")
}

fn nests_ordered(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "1. a\n2. b", "cmd-down tab")
}

fn adopts(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(
        stage,
        "- a\n  1. b\n  2. c\n  3. d",
        "cmd-up down end shift-tab",
    )
}

fn unnests(stage: &mut Stage) -> WindowHandle<AppShell> {
    typed(stage, "- a\n  - b\n  - ", "cmd-down enter")
}
