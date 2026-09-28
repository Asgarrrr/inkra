# Vault search, slice 4: opening at the hit — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md`,
S4 "Opening a hit" and "The editor"; the search spec's D10.

**Goal:** confirming a hit selects its range in the note and scrolls it into
view. In the note showing, without rebinding. In another note, the first
layout is shaped with the selection, and the reveal lands one frame later.

**Branch:** `vault-search-4-open` from `vault-search-3-picker` (`30dd332`).

## Success criterion

Written first, failing on `30dd332` (no `select_range`; a hit opens at the top):

`crates/app/src/shell/tests/vault_search.rs`, headless against `MemFs`, keys
through `simulate_keystrokes`:

- `a_hit_far_down_a_long_note_opens_selected_and_in_view`: `b` has 300 lines,
  `plan` on line 250. From `a`, `cmd-shift-f p l a n`, settle, `down enter`.
  Then: `showing == b`, the editor's selection is the hit's range, the scroll
  offset is below zero and the caret row lies inside the viewport
  (`EditorView::caret_in_view`, test-only), focus is in the editor.
- `a_hit_in_the_note_showing_selects_without_rebinding`: `a` showing, long,
  hit far down. The editor's `Buffer` entity id is the same before and after,
  an edit typed before the pick is still undoable (`cmd-z` restores it), and the
  selection is the hit's range, in view.
- `a_hit_past_the_end_of_an_edited_note_is_clipped`: `a` showing, its buffer
  shortened (an unsaved edit), then the picker emits `HitActivated` with the
  saved range, past the new end. No panic; the selection is clipped to the end.
- `picking_a_hit_then_another_drops_the_first`: `b`'s load held; `enter` on a
  hit in `b`, then `cmd-shift-f` and `enter` on a hit in `a` (showing). Release
  the gate: `showing == a`, the selection is `a`'s hit.
- The existing `enter_opens_the_note_of_the_selected_hit` keeps asserting focus
  in the editor after `enter`.

`editor/tests/scroll.rs`: `a_range_selected_right_after_a_rebind_is_revealed_after_the_first_paint`:
an `EditorView` rebound to 201 lines, `select_range` of a range on line 150
right after `open_buffer`, before any draw; after `run_until_parked`, the
scroll offset is exactly `offset_showing_from_below(150.)`.

Shot `vault-search-opened`: `step`, only in `roadmap.md` of the results
scene, from line 998, `enter`. Selection visible, the text scrolled.

## Data shapes

- `EditorView.restore_top: Option<TopLine>` becomes
  `scroll_to: Option<ScrollTo>`, `enum ScrollTo { Top(TopLine), Cursor }`, in
  `editor/reload.rs`. One slot, one consumer (`restore_scroll`, in `render`,
  before prepaint reads the offset). The spec's `pending_reveal` is
  `ScrollTo::Cursor`.
- `reveal_cursor`: with no geometry yet (no `last_text_top`, no layout, or a
  zero viewport), it sets `scroll_to = Some(Cursor)` instead of returning.
  Invariant: a reveal asked before the view has geometry lands once it has.
  No caller needs to know about the first paint.
- `TextElement::paint`: when `last_text_top` goes from `None` to `Some` and a
  `scroll_to` waits, `cx.defer_in(window, |_, _, cx| cx.notify())`. A notify
  inside the draw marks no frame (`invalidate_view` sets `dirty` only outside
  a draw phase), so it is deferred to the end of the effect cycle. One-shot per
  rebind: a zero viewport leaves `Cursor` waiting for the next render (a
  resize), with no loop.
- `on_buffer_reloaded`: `scroll_to.take().or_else(top_line → Top).map(mapped)`;
  `Cursor` maps to itself (the caret is mapped by `state.map_through`).
- `EditorView::select_range(range, window, cx)`, `pub(crate)`, in
  `editor/motion.rs`: `move_cursor` with `move_to(start, false)` then
  `move_to(end, true)`. `EditorState::place` already clips with
  `Buffer::clip`, so both ends are clipped; the head (caret) is the end.
- Shell: `show_note(id, at: Option<Range<ByteOffset>>, cx)` and
  `open_note(id, at, cx)`. Callers pass `None` except `HitActivated`.
  Showing branch: drop `activation`, `select_range` through
  `cx.with_window`, focus as today. `open_note`: after `open_buffer` or
  `EditorView::new`, in the same `update_in`, `select_range` with that
  `window`.

## Why render, not prepaint (D10's alternative)

Zed autoscrolls in `EditorElement::prepaint` because the editor element owns
its scroll. Here the scroll belongs to the parent `div`: by the time
`TextElement::prepaint` runs, the div has already applied the offset to its
children, so a reveal there lands a frame late too (`reveal_cursor`'s own
comment). The reload's `restore_scroll` already solves "scroll decided before
the geometry" in `render`; the reveal joins it.

## Steps

1. Tests above, failing.
2. `ScrollTo`, `reveal_cursor`'s pending branch, paint's deferred notify,
   `select_range`.
3. Shell `show_note`/`open_note` with `at`.
4. Shot scene `vault-search-opened`; render it and the editor scenes
   (`editor-caret`, `editor-selection`, `editor-concealed`, `reload-*`).
5. Docs: spec "As built (slice 4)", D10 note, roadmap, CLAUDE.md.

## Verification

- `./scripts/check.sh` green.
- `./scripts/test-names.sh` before/after: only additions.
- `cargo run -p inkra-app --features shot --example shot vault-search-opened`
  and the editor and reload scenes; read the PNGs.
- No change to `inkra-text` or `inkra-editor`: no `cargo mutants` needed.

## Red/blue on the plan

Red reviewed a partial tree; blue checked each claim against the finished one.

1. `restore_scroll` takes a pending `Cursor` before any paint and loses it.
   **Refuted**: `reveal_cursor` re-arms `Cursor` when it has no geometry,
   and removing the paint's deferred notify fails the editor test.
2. The showing branch has no `Window` while the picker is open.
   **Refuted**: `select_range` runs through `cx.with_window` before the focus
   match, whatever the modal.
3. `show_hit` renamed into `show_note(id, at)`. **Confirmed**, kept, recorded
   in the spec's "As built (slice 4)".
4. `ScrollTo` has two read sites (paint peeks, render takes). **Confirmed**,
   kept: paint only asks for the frame; `render` stays the one consumer.

## Red/blue on the diff

1. `reveal_cursor` with no geometry overwrote a waiting reload `Top` (a zero
   viewport after a paint, then a caret move). **Confirmed**, fixed with
   `get_or_insert`: a waiting `Top` wins, as it does when the reveal can
   scroll at once.
2. `show_note` looks the window up twice when no picker is open.
   **Confirmed**, kept: the focus lookup is the existing code.
3. `showing == Some` implies an editor, so the `Some(editor)` pattern is dead.
   **Refuted**: `NoteClosed` takes the editor; the pattern unwraps the
   `Option` either way.
4. The plan named the editor test differently. **Fixed** before blue read it.

## Follow-up: slice review

The coordinator's arbitration of the slice review, applied after `239bd34`:

1. `restore_scroll` reshapes before either request: a reload may land between
   the first paint and its deferred render. `reshape` now skips when
   `is_shaped_from` holds, so the common case costs no second shape.
2. `select_range` snaps outward to graphemes with a new
   `Buffer::clip_grapheme(offset, Bias)` in `inkra-text`, tested against a
   whole-text scan.
3. A hit lands centred (`Reveal::Center`, as Zed's `Autoscroll::center()`);
   the request carries its strategy, `ScrollTo::Cursor(Reveal)`.
   `request_scroll` is the one way in.
4. `ScrollTo`, `Reveal`, `request_scroll` and `restore_scroll` live in
   `editor/scroll.rs`; `TopLine` and `restore_top` stay in `reload.rs`.
5. to 9. Comments, the clip test (`alpha pl`, 6..8), the shot text, the
   editor rule, the roadmap item and the known limit.

Red/blue on the follow-up:

1. A waiting reload `Top` overrode a hit centred at once. **Confirmed**,
   fixed: a reveal replaces any request, a `Top` only fills an empty slot,
   and `reveal_cursor` requests instead of scrolling while any request waits,
   since the layout may predate the text. This changes the coordinator's
   "a waiting request wins" for a reveal over a `Top`.
2. The rule was untested (the test never scrolled, so no `Top` competed).
   **Confirmed**, fixed: `scrolled_with_a_reload` and two tests; the
   `get_or_insert` rule, the missing filter and the missing reshape each fail
   one.
3. The optional `if let` → `match` in `open_note`: clippy's
   `single_match_else` rejects it; reverted.
4. `clip_grapheme` swallows `GraphemeIncomplete`, as its neighbours do; the
   whole text is one chunk. Kept.
5. The proptest's Left oracle read `clip`'s output. **Fixed**: raw offset.
