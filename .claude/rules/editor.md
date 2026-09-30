---
paths:
  - "crates/app/src/editor/**"
  - "crates/app/src/field.rs"
  - "crates/app/src/text_input.rs"
  - "crates/app/src/text_input/**"
  - "crates/editor/**"
---

# Editor view

`EditorView` and `NameField` host one `TextInput` (`text_input.rs`); the gates
are `InputHost`'s, and new actions go through them:

- `edit_buffer` — the only path that mutates text. It leases with `as_mut`
  (notifies on drop), drops the lease, then calls `after_edit`. The editor's
  reshapes with the style stored in `DocumentLayout` before revealing the
  caret: outside a draw, `window.text_style()` is gpui's default.
- `move_cursor` — caret moves; reads the buffer, notifies the view, then
  `after_move`. The editor's reshapes only when the move changes which
  markdown elements the selection touches, so `reveal_cursor` scrolls by the
  revealed layout.
- `unmark_text` — the one `EditorState` change that is not a text change; no
  hook runs.
- `open_buffer` — the only path that rebinds the buffer (`TextInput::rebind`). Never swap text through
  `edit_buffer`: it would land on the previous note's undo stack.
- A key both hosts need binds once, in `TextInput`, with its handler in
  `text_input/actions.rs`; a host binds only its own keys
  (`keymap/tests.rs` fails on a key bound in both).
- `InputAdapter` stands in for gpui's `ElementInputHandler`: a protocol call
  gpui adds to `InputHandler` needs forwarding there, or it silently takes the
  trait default.
- The editor's scroll geometry is `Viewport`'s (`editor/viewport.rs`). A
  scroll decided before its geometry exists goes through `Viewport::request`,
  which `TextElement`'s prepaint resolves against the frame's layout; never a
  second field. `open_buffer` resets it by value, `Viewport::default()`.
- `open_buffer` takes an `Option<ViewState>` and restores it without a reveal;
  `view_state()` reads a waiting scroll request before the drawn frame.
- A window point becomes an offset only through `TextInput::hit_test`:
  `last_bounds` is already scrolled, so no caller subtracts the offset.
- `copy` takes no gate: `as_mut` notifies unconditionally and would schedule an
  autosave for a `⌘C`.

Layout:

- Every pixel ↔ offset conversion goes through `DocumentLayout`, in rows. All its
  lookups clamp.
- An offset at a wrap belongs to the next row; End, clicks and up/down landing
  on the earlier row clamp to `Row::caret_end`. That is why the caret has no
  affinity bit. Row starts snap to grapheme boundaries.
- The measure closure reuses `last_layout` when `DocumentLayout::is_shaped_from`
  holds, so a keystroke shapes once (`editor/tests/shapes.rs`). An input that
  changes the shaped text must join that check. Intrinsic-width queries shape at
  the last width: a second width costs a whole shape. So the element needs an
  ancestor that gives it a definite width, never a shrink-to-fit one.
- The measure closure reports zero min-content width; the column there would
  become the flex minimum and overflow a narrow pane (`shell/tests/column.rs`).
- `Selection::set_head` / `collapse_to` take a `Goal`; only vertical moves pass
  a real one.
- No observer of an `Entity<Buffer>` may lease that same buffer: a second lease
  panics.
- Two panes on one note are not supported: `Selection` holds raw offsets, not
  anchors.
