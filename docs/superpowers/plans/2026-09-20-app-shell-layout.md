# App Shell Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the window's root view a small `AppShell` that composes a
sidebar placeholder and the editor, instead of `EditorView` filling the whole
window directly.

**Architecture:** `AppShell` holds `editor: Entity<EditorView>` as one named,
opaque slot (no sidebar field — nothing toggles it yet, so it's markup, not
state) and delegates `Focusable::focus_handle` to that entity, mirroring
`Workspace::focus_handle` in Zed's vendored source
(`vendor/zed-reference/crates/workspace/src/workspace.rs:9529-9533`). Full
rationale: `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`.

**Tech Stack:** Rust, gpui (vendored path dependency, see project `CLAUDE.md`
for why — never the crates.io copy).

**Branch:** `app-shell-layout` (already created; the design spec is already
committed on it).

**Revision note:** this plan went through an adversarial review (a red team
attacking it for bugs, a blue team looking for improvements) before
implementation started. Both independently flagged the same two blocking
lint failures and the same unnecessary visibility change; both are fixed
below. Original v1 had a separate "Task 1: widen `EditorView::cursor` to
`pub(crate)`" purely to support a test assertion — deleted, because the test
already owns the buffer entity directly and can assert on it with no
visibility change at all.

---

### Task 1: Add `AppShell` and make it the window root

Done as one task, one commit, deliberately: introducing `AppShell` without
wiring it into `main.rs` leaves it referenced only from `#[cfg(test)]`, which
makes the plain (non-test) compilation of `inkra-app` see the whole type as
dead code — `rustc`'s `dead_code` lint, promoted to a hard error by
`check.sh`'s `-D warnings`. Splitting this into two commits would mean the
first one fails full verification on its own, which this project's
commit-hygiene expects to hold at every commit.

TDD still applies within the task: the test is written against `AppShell`
before it exists, so the first run fails to *compile* — that is the expected
"red" for a Rust type that doesn't exist yet, not a runtime assertion
failure.

**Files:**
- Create: `crates/app/src/shell.rs`
- Modify: `crates/app/src/main.rs`
- Modify: `CLAUDE.md` (don't-recreate table)

- [ ] **Step 1: Register the module**

Current code (`main.rs:6-7`):

```rust
mod editor;
mod keymap;
```

Change to:

```rust
mod editor;
mod keymap;
mod shell;
```

- [ ] **Step 2: Write the failing test**

Create `crates/app/src/shell.rs`:

```rust
//! The window's root view: composes the sidebar and the editor.
//!
//! Named, opaque slots rather than an ad hoc `div` tree assembled at render
//! time — see `docs/superpowers/specs/2026-09-20-app-shell-layout-design.md`
//! for why, and what this deliberately leaves out (sidebar content, resize,
//! a status bar, multi-pane).

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render, Styled,
    Window, div, px, rgb,
};

use crate::editor::EditorView;

pub(crate) struct AppShell {
    editor: Entity<EditorView>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use gpui::{TestAppContext, VisualTestContext};
    use inkra_text::Buffer;

    use super::{AppShell, EditorView};
    use crate::keymap::key_bindings;

    /// The one regression this file exists to guard: the existing
    /// `simulate_keystrokes` tests in `editor/tests.rs` open a window with
    /// `EditorView` as the direct root, so they never exercise `AppShell` at
    /// all. Only this test opens the window the way `main.rs` actually does
    /// — through `AppShell` — and proves a keystroke still reaches the
    /// editor once it is no longer the window root.
    #[gpui::test]
    fn a_keystroke_through_app_shell_reaches_the_editor(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let buffer = cx.update(|cx| cx.new(|_| Buffer::new("")));
        let editor = cx.update(|cx| cx.new(|cx| EditorView::new(buffer.clone(), cx)));
        let window = cx.add_window(move |_, _| AppShell::new(editor));
        window
            .update(cx, |shell, window, cx| {
                window.focus(&shell.focus_handle(cx), cx);
            })
            .unwrap();

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_input("hi");

        window
            .update(&mut visual, |_, _, cx| {
                assert_eq!(buffer.read(cx).text(), "hi");
            })
            .unwrap();
    }
}
```

Note what this test does *not* need: no visibility change anywhere in
`editor/mod.rs`. It builds `buffer: Entity<Buffer>` itself and keeps a
handle to it, so it reads the result straight from the buffer — the same
thing `editor/tests.rs`'s own tests assert on — rather than reaching into
`EditorView`'s private state.

- [ ] **Step 3: Run the test, confirm it fails to compile**

Run: `cargo test -p inkra-app a_keystroke_through_app_shell_reaches_the_editor`
Expected: FAIL — compile error, e.g. `cannot find type 'AppShell' in this
scope` / `no function or associated item named 'new' found for struct
'AppShell'`. Confirms the test is actually exercising code that doesn't
exist yet.

- [ ] **Step 4: Implement `AppShell`**

Append to `crates/app/src/shell.rs`, after the `pub(crate) struct AppShell`
definition and before the `#[cfg(test)]` block:

```rust
impl AppShell {
    /// Takes an already-built editor: `AppShell` composes the window, it
    /// does not decide what buffer opens in it.
    pub(crate) fn new(editor: Entity<EditorView>) -> Self {
        Self { editor }
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for AppShell {
    // Matches EditorView::render's own allow (editor/mod.rs): RRGGBB hex
    // literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .child(
                // Sidebar placeholder: fixed width, no content, nothing
                // toggles it yet. flex_none (grow 0, shrink 0) so it never
                // competes with the editor for space.
                div()
                    .flex_none()
                    .w(px(180.))
                    .h_full()
                    .bg(rgb(0x181818)),
            )
            .child(
                // flex_1 (grow 1, shrink 1, basis 0%) makes this wrapper
                // claim all remaining row width — gpui/styled.rs:181-186.
                // EditorView's own .size_full() then fills 100% of this
                // wrapper's resolved size; EditorView never needs to know
                // it's no longer the window root.
                div().flex_1().h_full().child(self.editor.clone()),
            )
    }
}
```

`AppShell` is still unreferenced outside `#[cfg(test)]` at this point — that
is expected and fixed in Step 6, in the same commit.

- [ ] **Step 5: Run the test, confirm it passes**

Run: `cargo test -p inkra-app a_keystroke_through_app_shell_reaches_the_editor`
Expected: PASS (1 passed; 0 failed).

- [ ] **Step 6: Wire `main.rs` to open `AppShell` as the window root**

Update the module doc comment. Current code (`main.rs:1-4`):

```rust
//! Inkra: a note editor on GPUI.
//!
//! This file is the bootstrap and nothing else. Editing semantics live in
//! `inkra-editor`, the text model in `inkra-text`, and the view in `editor/`.
```

Change to:

```rust
//! Inkra: a note editor on GPUI.
//!
//! This file is the bootstrap and nothing else. Editing semantics live in
//! `inkra-editor`, the text model in `inkra-text`, the editor view in
//! `editor/`, and the window's outer layout in `shell.rs`.
```

Import `AppShell`. Current code (`main.rs:12`):

```rust
use editor::EditorView;
```

Change to:

```rust
use editor::EditorView;
use shell::AppShell;
```

Build `AppShell` as the window root. Current code (`main.rs:23-47`):

```rust
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                // Multi-line, and taller than the window, so that starting the
                // app exercises per-line layout and the scroll container —
                // neither of which any test renders.
                let buffer = cx.new(|_| {
                    Buffer::new(
                        "héllo 😀 — type here\n\
                         Enter splits a line, up and down keep their column.\n\
                         \n\
                         An empty line above.\n\
                         A much longer line, so the ragged right edge has \
                         something to be ragged about.\n\
                         short\n\
                         six\nseven\neight\nnine\nten\neleven\ntwelve\n\
                         thirteen — past the window, so scrolling has to work",
                    )
                });
                cx.new(|cx| EditorView::new(buffer, cx))
            },
        );
```

Change to:

```rust
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                // Multi-line, and taller than the window, so that starting the
                // app exercises per-line layout and the scroll container —
                // neither of which any test renders.
                let buffer = cx.new(|_| {
                    Buffer::new(
                        "héllo 😀 — type here\n\
                         Enter splits a line, up and down keep their column.\n\
                         \n\
                         An empty line above.\n\
                         A much longer line, so the ragged right edge has \
                         something to be ragged about.\n\
                         short\n\
                         six\nseven\neight\nnine\nten\neleven\ntwelve\n\
                         thirteen — past the window, so scrolling has to work",
                    )
                });
                let editor = cx.new(|cx| EditorView::new(buffer, cx));
                cx.new(|_| AppShell::new(editor))
            },
        );
```

Nothing below this block changes: the post-open focus call
(`window.update(cx, |view, window, cx| { window.focus(&view.focus_handle(cx),
cx); cx.activate(true); })`) already calls `Focusable::focus_handle`
generically on whatever `view` is — it now resolves through `AppShell`'s
impl instead of `EditorView`'s, with no edit needed.

- [ ] **Step 7: Record `AppShell` in the don't-recreate table**

Per this project's own memory rule: an architecture fact that future
sessions need gets written to `CLAUDE.md` in the same change that introduces
it, not left to be rediscovered.

Current code (`CLAUDE.md`, last row of the don't-recreate table):

```markdown
| Restore the gpui source we build | `./scripts/vendor-gpui.sh` | `scripts/` |
```

Add a row directly after it:

```markdown
| Restore the gpui source we build | `./scripts/vendor-gpui.sh` | `scripts/` |
| The window's root view (sidebar slot + editor) | `AppShell` | `app/src/shell.rs` |
```

- [ ] **Step 8: Build and run the full workspace verification**

Run: `./scripts/check.sh`
Expected: exits 0 — `cargo fmt --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings` (this is what step 4's two
`#[allow(...)]`s and this step's full wiring are for — without either, this
is where it would first fail), the gpui-isolation check, `cargo test
--workspace` (including every existing `simulate_keystrokes` test in
`app/src/editor/tests.rs`, unaffected since they still open `EditorView`
directly, and the new `a_keystroke_through_app_shell_reaches_the_editor`),
and `cargo build --release`.

- [ ] **Step 9: Commit**

```bash
git add crates/app/src/shell.rs crates/app/src/main.rs CLAUDE.md
git commit -m "Add AppShell as the window root

EditorView is no longer the window root; it's AppShell's one named,
opaque slot. Focusable delegates to it, mirroring
Workspace::active_pane delegation in vendor/zed-reference. Sidebar is
a fixed-width placeholder -- no state, since nothing toggles it yet."
```

---

### Task 2: Full verification pass

`check.sh` already ran clean at the end of Task 1. This task is the
project's required manual step on top of that: `check.sh` renders nothing,
so a visual regression in a layout change would pass every automated check
and still be wrong on screen.

**Files:** none.

- [ ] **Step 1: Run the app and look at it**

Run: `cargo run -p inkra-app` (background task, so it can be left running
while inspecting the window).

Confirm, by looking at the window:
- A ~180px dark column (`#181818`) on the left, full window height, empty.
- The editor fills the remaining width — not collapsed to a sliver, not
  overlapping the sidebar. This is what Task 1 Step 4's `.flex_1()` wrapper
  is verified (`gpui/styled.rs:181-186`, `:208-213`) to produce; this step
  confirms it visually, not just on paper.
- Typing still works: click in the editor area and type a character.

Stop the background run once confirmed.

- [ ] **Step 2: No commit**

This task produces no diff. If Step 1 surfaces a problem, fix it as a new,
separate commit — do not fold an unplanned fix silently into Task 1's
commit.
