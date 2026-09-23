---
paths:
  - "crates/app/src/editor/**"
  - "crates/app/src/field.rs"
  - "crates/editor/**"
---

# Editor view

`EditorView` has four gates; new actions go through them:

- `edit_buffer` — the only path that mutates text. It leases with `as_mut`
  (notifies on drop), then reshapes with the style stored in `DocumentLayout`
  before revealing the caret: outside a draw, `window.text_style()` is gpui's
  default.
- `move_cursor` — caret moves; reads the buffer, notifies the view, and
  reshapes only when the move changes which markdown elements the selection
  touches, so `reveal_cursor` scrolls by the revealed layout.
- `update_state` — `EditorState` changes that must not look like a text change
  (`unmark_text`).
- `open_buffer` — the only path that rebinds the buffer. Never swap text through
  `edit_buffer`: it would land on the previous note's undo stack.
- `copy` takes no gate: `as_mut` notifies unconditionally and would schedule an
  autosave for a `⌘C`.

Layout:

- Every pixel ↔ offset conversion goes through `DocumentLayout`, in rows. All its
  lookups clamp.
- An offset at a wrap belongs to the next row; End, clicks and up/down landing
  on the earlier row clamp to `Row::caret_end`. That is why the caret has no
  affinity bit. Row starts snap to grapheme boundaries.
- The measure closure reports zero min-content width; the column there would
  become the flex minimum and overflow a narrow pane (`shell/tests/column.rs`).
- `Selection::set_head` / `collapse_to` take a `Goal`; only vertical moves pass
  a real one.
- No observer of an `Entity<Buffer>` may lease that same buffer: a second lease
  panics.
- Two panes on one note are not supported: `Selection` holds raw offsets, not
  anchors.
