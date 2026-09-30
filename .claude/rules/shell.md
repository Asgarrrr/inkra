---
paths:
  - "crates/app/src/shell.rs"
  - "crates/app/src/shell/**"
  - "crates/app/src/sidebar/**"
  - "crates/app/src/keymap.rs"
  - "crates/app/src/bootstrap.rs"
---

# Shell, sidebar, keymap

- `AppShell.bound` holds the editor and its note in one `Option`: no editor
  until a note opens, so nothing untracked can be typed into; an empty tab has
  no editor either. `render_empty` fills the slot. `set_showing` is its one writer.
- Every tab action dismisses the picker first (`dismiss_modal`).
- An action holds its window leased: `cx.with_window` returns `None` inside it,
  so tab code takes `&mut Window` (`unfocus_editor`). A subscription fired by
  a tab action runs after the lease ends, so `with_window` works there.
- `AppShell` tracks its own `FocusHandle` on the root div; without it, with no
  editor, `cmd-s` never reaches its handler.
- `AppShell::open_note` gets its `Window` via `update_in`; do not thread a
  `&mut Window` through `bootstrap::root`. The bind, `set_current` and the focus
  move happen in one `update_in`, with no `await` between them.
- A widget that opens a note emits `NoteActivated`; it never holds an
  `Entity<EditorView>`. The emit is indirect because a synchronous callback would
  re-lease the `Sidebar`.
- Every binding names a context (only `cmd-q` does not). A new focusable view
  inherits nothing and must bind every key it needs, as `NameField` does.
- A binding goes where its handler is reachable from the focused leaf: window-wide
  actions (`cmd-s`, `cmd-n`) on `AppShell`; `Trash`/`Rename` on the tree, which
  owns the selection.
- `Sidebar::selected` is what a tree command acts on; `NoteStore::current` is
  what the editor shows. Do not confuse them.
- A chrome row states its `UI_LINE_HEIGHT` and takes no height of its own, so it
  cannot clip its text.
