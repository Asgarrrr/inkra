---
paths:
  - "crates/app/src/shell.rs"
  - "crates/app/src/shell/**"
  - "crates/app/src/sidebar/**"
  - "crates/app/src/keymap.rs"
  - "crates/app/src/bootstrap.rs"
---

# Shell, sidebar, keymap

- `AppShell.shown` (`Shown`) is `Empty`, `Bound` (the editor and its note),
  `Image` (an image file, drawn from `Images`, with no editor), `Trashing` (the
  tab stays on a note whose trash is pending, with no editor) or `View`.
  No editor until a note opens, so nothing untracked can be typed into.
  `render_empty` fills the slot. `set_shown` is its one writer. A note leaves
  the tabs only on `NoteClosed`; `TrashFailed` rebinds a `Trashing` tab.
- Every tab action dismisses the picker first (`dismiss_modal`).
- One modal at a time: a picker or a `ContextMenu`. A right press that opens a
  menu calls `prevent_default`, or the root takes the focus. `change_layout`
  closes an open menu.
- An inactive tab's slot in `Tabs<Place, Parked>` holds its `Parked` view, so
  each op's closure type picks `Parked::park` or `capture`; bind a `Parked` only
  through `unpark`, or its anchors leak.
- A drop in the bar (`move_tab`) calls no closure: it captures, parks and
  unparks nothing, and keeps `activation` (T4). It does not activate the tab
  dropped. A tab's left press calls `prevent_default`, so the root does not
  take focus from the editor. An `IconButton` (a dock toggle, the ×, the `+`, the sidebar's search and new-note buttons, the strip's history arrows) prevents it on
  a left press itself, so a click on one leaves focus where it was.
- A disabled `IconButton` drops its click and is drawn as `{id}-disabled`, which tests read; the arrows' state comes from `Tabs::can_go_back`/`can_go_forward`, never computed in the view.
- Every title-row segment (`title-left`, `title-tabs`, `title-right`) and a lower group's header call `ui::keep_focus` on a left press, so a press on their gaps leaves the editor focused.
- The window moves only from the four leaf drag regions of the title row
  (`title-drag-start`, `title-drag-left`, `title-drag-end`, `title-drag-right`): only a region's left press sets
  `title_drag_armed`, and calls `prevent_default` to keep the editor's focus.
  The root's mouse move clears the flag on every move, and starts the move only with the
  left button held: a release outside the window reaches no listener. Never put
  a drag handler on the row: a tab's press would bubble to it.
- With no vault, no dock is drawn; the note column starts at x 0.
- `AppShell.layout` changes only through `change_layout`: it lands focus when Files stops being visible and cancels an open name field.
- A drop on a dock icon moves the view to that icon's index; a drop on a dock column or a drop strip appends (`AppShell::append`). Both land in `move_tab_to`, which goes through `change_layout` across groups and `move_tab` within one.
- `Tabs` holds no place for the active tab (H8): `showing()` is the file it
  draws (`Bound`'s note or an `Image`, `None` while `Trashing`; `bound_note()`
  is the editor's note), and
  `leaving` hands `Tabs` the place when the tab is left.
- An action holds its window leased: `cx.with_window` returns `None` inside it,
  so tab code takes `&mut Window` (`unfocus_editor`). A subscription fired by
  a tab action runs after the lease ends, so `with_window` works there.
- `AppShell` tracks its own `FocusHandle` on the root div; without it, with no
  editor, `cmd-s` never reaches its handler.
- `AppShell::open_file` gets its `Window` via `update_in`; do not thread a
  `&mut Window` through `bootstrap::root`. The bind, `set_current` and the focus
  move happen in one `update_in`, with no `await` between them.
- A widget that opens a file emits `FileActivated`; it never holds an
  `Entity<EditorView>`. The emit is indirect because a synchronous callback would
  re-lease the `Sidebar`. `FileActivated` and `Open { Here, NewTab }` live in
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
- A picker or backlink row states its `UI_LINE_HEIGHT` and takes no height of its
  own, so it cannot clip its text; a tree row is `ListItem::inset`,
  `TREE_ROW_HEIGHT` tall, the pitch `row_center` reads.
- The restore lands in `Restore`'s `Landing`, never in `activation`: a user open
  replaces `activation` and must not cancel it. It applies only while the user
  holds nothing (one tab, `Shown::Empty`, no `activation`, no closed tab). It
  loads every restored note first: every tab's note has a buffer until `NoteClosed`.
- The session has one writer, `write_session` (its `SessionWriter`), gated on
  `Restore::Done`; arm it with `save_session_later`, never write from elsewhere.
- The session file is read once per shell, before the shell is built: by `bootstrap::read_session` at launch
  (its frame places the window; `block_on`, so a launch test never gates it), by the open's step 3 after
  (awaited and bounded, so an open test may gate it). `AppShell::new` takes the `Opened`.
- A tree, picker or backlink row is a `ui::ListItem`, a tab is a `ui::Tab`, and an
  icon-only button is a `ui::IconButton`: each owns background, hover and padding; the
  view attaches handlers, role, labels and content. A caller's `.hover()` on any of them
  trips gpui's "hover style already set" assert.
- `AppShell.leaving` (`Leave`) changes only through `leave.rs` (`start_quit`, `start_open`, `stop_open`, `stop_leaving`); one open task is alive at a time (`opening_vault`), so `Leave::Open` under way is always that task's own, and the task reads it at each step and stops once it is gone; `settle` refuses a leave that is not under way; a quit takes over an open.
