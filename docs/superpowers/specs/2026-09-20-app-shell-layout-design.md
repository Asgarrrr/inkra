# App shell layout — design

Today `EditorView` is the window's root view (`crates/app/src/main.rs:23-47`):
it fills the whole window, with no chrome around it. The application is a
vault of notes with a sidebar, panes, and search — none of which exist yet,
and none of which this design builds. This design gives the window a shell
shaped so those land later without restructuring the root view again.

## Why now, and why this shape

`main.rs`, `keymap.rs`, and `editor/` are the only things under
`crates/app/src/`. Nothing currently distinguishes "the window's outer
layout" from "the editor". Adding a sidebar today means either bolting a
`div` onto `EditorView::render` (mixing editor rendering with window
composition) or introducing a real root view. The second is the smaller
change and the one that does not need to be undone later.

Zed's `Workspace` (`vendor/zed-reference/crates/workspace/src/workspace.rs`)
was read for the shape of this seam, not for its content — Zed's docks
support multiple panels, persisted resize, and per-dock layout policy, none
of which apply here. What transfers:

- **Named, opaque slots.** `Workspace` holds `docks`, `center`, `status_bar`,
  `titlebar_item` as fields — never an ad hoc `div` tree assembled at render
  time. The shell composes slots; it does not know what is inside them.
- **Focus delegates to content, not to the shell.** `Workspace::focus_handle`
  (`workspace.rs:9529-9533`) returns `self.active_pane.focus_handle(cx)`,
  using the blanket `impl<V: Focusable> Focusable for Entity<V>`
  (`gpui/window.rs:706`). The shell is never itself a focus target.

What does not transfer, and is deliberately left out: multiple panels per
dock, resize, open/closed state (nothing toggles it yet — an unread field is
dead state), persistence, and a status bar. Each is a one-field or one-slot
addition when something needs it, not a cost paid now.

## Architecture

New file `crates/app/src/shell.rs`:

```rust
pub(crate) struct AppShell {
    editor: Entity<EditorView>,
}

impl AppShell {
    pub(crate) fn new(editor: Entity<EditorView>) -> Self { .. }
}

impl Focusable for AppShell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for AppShell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .child(/* sidebar */)
            .child(self.editor.clone())
    }
}
```

`AppShell` holds `editor: Entity<EditorView>` as its one named slot — opaque,
per the pattern above. No `sidebar` field yet: the sidebar has no state to
hold (fixed width, no content, nothing toggles it), so it is markup in
`render`, not a field.

`main.rs` changes minimally: it still builds `Buffer` → `EditorView` exactly
as today, then wraps the result in `AppShell::new(editor)` as the
window's root view instead of using `EditorView` directly. The post-open
focus call (`main.rs:51-54`) is unchanged — it already calls
`view.focus_handle(cx)` generically, and now resolves through `AppShell`'s
delegating impl instead of `EditorView`'s own.

## Visual

- Sidebar: fixed **180px** width, background `0x181818` (darker than the
  editor's `0x1e1e1e`), empty — no placeholder text, so nothing has to be
  removed when it grows content.
- `EditorView`'s own `.p_4()` (`editor/mod.rs:305`) is untouched — it is the
  text area's internal padding, not a window-level margin. `AppShell` adds no
  padding of its own; the sidebar and the editor sit flush against the
  window edge and against each other.
- Layout: `div().flex().flex_row().size_full()`, two children, `[sidebar]
  [editor]`, in that order.
- **To verify during implementation, against vendored gpui source, not
  assumed:** `EditorView::render` already calls `.size_full()`
  (`editor/mod.rs:298`), previously correct because it was the window root.
  As a flex row child it may need `.flex_1()` instead so it claims the
  remaining width rather than the sidebar's fixed basis competing on equal
  terms — check `gpui/elements/div.rs` flex-resolution before assuming
  either form is right, and confirm visually (`cargo run -p inkra-app`).

## Testing

No editing semantics change, so nothing new in `text` or `editor`.

- **Existing `simulate_keystrokes` tests keep passing, but prove nothing new
  here:** they build their window with `EditorView` as the direct root
  (`editor/tests.rs`'s `window_on`), never through `AppShell`. Green here only
  shows editing semantics are unaffected by the wrap — it cannot catch a
  broken `AppShell::focus_handle`, because it never runs.
- **New, and the actual regression proof:** one `#[gpui::test]` that opens a
  window with `AppShell` as the root (mirroring `main.rs`: build `Buffer` →
  `EditorView` → `AppShell`), focuses it the same way `main.rs` does —
  `window.focus(&view.focus_handle(cx), cx)` — then calls
  `cx.simulate_keystrokes(...)` and asserts the buffer changed. This is the
  only test that actually dispatches a keystroke through `AppShell`'s
  `Focusable` impl; a wrong or missing delegation strands every keystroke
  silently, and only this test would notice.
- No snapshot or visual test, per the project's testing rule: the UI is
  still unstable. A manual render pass (`cargo run -p inkra-app`) after
  implementation stands in, per `check.sh` rendering nothing.

## Out of scope

Sidebar content, open/closed state, resize, a status bar, a titlebar, and
multi-pane. Each has a named trigger in `docs/roadmap.md` or the modular
architecture design doc; none has fired yet. This design's only job is to
make the window's root view a shell instead of the editor, on a shape that
survives those triggers firing later.
