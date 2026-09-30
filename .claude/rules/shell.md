---
paths:
  - "crates/app/src/shell.rs"
  - "crates/app/src/shell/**"
  - "crates/app/src/sidebar/**"
  - "crates/app/src/keymap.rs"
  - "crates/app/src/bootstrap.rs"
---

# Shell, sidebar, keymap

- `AppShell.shown` (`Shown`) is `Empty`, `Bound` (the editor and its note) or
  `Trashing` (the tab stays on a note whose trash is pending, with no editor).
  No editor until a note opens, so nothing untracked can be typed into.
  `render_empty` fills the slot. `set_shown` is its one writer. A note leaves
  the tabs only on `NoteClosed`; `TrashFailed` rebinds a `Trashing` tab.
- Every tab action dismisses the picker first (`dismiss_modal`).
- Park only in the `leaving` closure of `Tabs::insert` and `Tabs::activate`,
  and in `land_restore` (`shell/session.rs`), which parks each restored inactive tab;
  `close`, `commit`, `back` and `forward` keep the plain `capture`: back and
  forward keep the tab active, so they never park. Unpark every place
  `activate` and `close` return (`arrive`) and every closed id; a place `back`
  or `forward` returns is a plain view. Drop a note's entries with
  `forget_note`, never `tabs.forget` alone. `Tabs` holds no view (I2).
  `close_tab` unparks the closed id **before** `close` and hands the view to
  `resolve`: the closed stack keeps plain offsets. `reopen` parks the tab left
  through `insert`'s closure; its place is a plain view, never `arrive`d.
- A drop in the bar (`move_tab`) calls no closure: it captures, parks and
  unparks nothing, and keeps `activation` (T4). It does not activate the tab
  dropped. A tab's left press calls `prevent_default`, so the root does not
  take focus from the editor.
- `Tabs` holds no place for the active tab (H8): `active_note()` is its note
  (`showing()` is the note bound in the editor, `None` while `Trashing`), and
  `leaving` hands `Tabs` the place when the tab is left.
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
  re-lease the `Sidebar`. `NoteActivated` and `Open { Here, NewTab }` live in
  `open.rs`; every opener carries an `Open`, and `AppShell::open` routes it.
- A new-tab open (`open_in_new_tab`) inserts its tab when the note lands, in the
  one `activation` slot, and parks the tab left through `insert`'s closure. It
  lands only while the tab it started from is active.
- Every binding names a context (only `cmd-q` does not). A new focusable view
  inherits nothing and must bind every key it needs, as `NameField` does.
- A binding goes where its handler is reachable from the focused leaf: window-wide
  actions (`cmd-s`, `cmd-n`) on `AppShell`; `Trash`/`Rename` on the tree, which
  owns the selection.
- `Sidebar::selected` is what a tree command acts on; `NoteStore::current` is
  what the editor shows. Do not confuse them. A tree gesture writes `selected`
  at once through `Sidebar::select`, which bumps its `cursor`. `set_shown`
  moves it to the note showing through `follow(id, since)`, only while the
  cursor is still `since`: a synchronous op reads `since` in its own update;
  an async landing captures it at spawn. `None` and a name being typed leave
  it.
- A chrome row states its `UI_LINE_HEIGHT` and takes no height of its own, so it
  cannot clip its text.
- The restore lands in `Restore`'s `Landing`, never in `activation`: a user open
  replaces `activation` and must not cancel it. It applies only while the user
  holds nothing (one tab, `Shown::Empty`, no `activation`, no closed tab). It
  loads every restored note first: every tab's note has a buffer until `NoteClosed`.
- The session has one writer, `write_session` (its `SessionWriter`), gated on
  `Restore::Done`; arm it with `save_session_later`, never write from elsewhere.
