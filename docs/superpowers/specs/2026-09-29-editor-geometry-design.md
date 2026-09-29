# One owner for editor layout and scroll geometry

Date: 2026-09-29. Status: **built** (As built, at the end).

A refactor from the architecture review. Behaviour stays identical. A
keystroke still shapes the note once (`docs/roadmap.md`, Measurements).

## Context

Checked against `arch/deepening` at `e108302`. The brief was imprecise in
three places, corrected here.

The editor's geometry is five pieces of state in two owners:

- `TextInput` (`text_input.rs:63-64`) holds `last_layout` and `last_bounds`.
  They are shared with `NameField`: the IME protocol
  (`bounds_for_range`, `character_index_for_point`, `text_input.rs:268-299`)
  and `Home`/`End` (`text_input/actions.rs:86-103`) read them in both hosts.
  They are not the editor's alone, so they stay in `TextInput`.
- `EditorView` (`editor/mod.rs:42-49`) holds `last_text_top`, `scroll_handle`
  and `scroll_to`. Only the editor scrolls.

The rules that keep them consistent live in comments, spread over six files:

- `mod.rs:42-46`: never re-derive the text top from `last_bounds` and a live
  `offset()`, which counts the scroll twice. `last_bounds` is already
  scrolled, so no hit test subtracts the offset.
- `mod.rs:89-92`: `open_buffer` clears three fields by hand; `rebind` clears
  the other two (`text_input.rs:86-91`). A field added later can be missed.
- `motion.rs:33-60`, `reveal_cursor`: runs from the gates, not prepaint. It
  scrolls only with a text top, a layout, a non-zero viewport and no waiting
  request; otherwise it queues `ScrollTo::Cursor`.
- `scroll.rs:41-58`: the request policy, and `restore_scroll` reshaping
  before it applies.
- `reload.rs:39-84`: the top line is read from the old layout, mapped through
  the diff, and restored once the new text is laid out. `viewport_top` is a
  second derivation of the same text-top rule.
- `element.rs:190-199`: `paint` writes `last_text_top` and defers a notify on
  the first paint when a scroll waits.

Pixel to offset, from a window point, is written three times:
`mouse.rs:9-12` (`position_for_point`), `mouse.rs:18-19` (`link_under`) and
`text_input.rs:293-296` (`character_index_for_point`).

`move_vertically` (`motion.rs:62-110`) computes its target from
`DocumentLayout` alone: the caret's row, the goal x, the next row in the
direction that holds a caret, the offset at that x. It runs only through the
view, so its tests need a window, a view and keystrokes (`tests/wrap.rs`).

Corrections to the brief:

- `move_cursor_revealing` no longer exists. `select_range` calls
  `move_cursor`, then `reveal_cursor(Center)` (text-input spec, As built).
- "`edit_buffer` drops the lease before reshape" is `InputHost`'s I1, already
  enforced in one place (`text_input.rs:169-173`). It is not geometry and
  does not move.
- "`reshape` calls `refresh_markdown` first" is about the markdown, which
  the view owns. It stays in `reshape`.

## What Zed does

- `ScrollManager` (`editor/src/scroll.rs:139`) is a plain struct the `Editor`
  owns: the scroll anchor, the vertical margin and one `autoscroll_request`
  slot. Callers `request_autoscroll`; `EditorElement::prepaint` calls
  `take_autoscroll_request` and applies it against the frame's layout
  (`scroll.rs:469`, `scroll/autoscroll.rs:127`).
- `movement::up` and `movement::down` (`movement.rs:84-117`) are free
  functions over a `DisplaySnapshot`, a start, a `SelectionGoal` and layout
  details. They return the new point and goal. `navigation.rs:66` applies
  the result. The pure part is tested without the view.
- `PositionMap::point_for_position` (`element.rs:10813`) is the one window
  point to buffer position conversion. It lives in the element's prepaint
  output.

Deviations:

- **The scroll offset stays in gpui's `ScrollHandle`, in pixels.** The parent
  `div` owns the scroll, not our element. Zed's anchor needs an element that
  owns its scroll: roadmap item "`TextElement` owns its vertical scroll",
  out of scope.
- **The request is applied in `render`, not prepaint.** Same reason; see the
  vault-search spec, As built (slice 4).
- **A reload's top line only fills an empty slot.** Zed's request replaces
  always. Ours keeps the existing policy.
- **Reset by value.** Zed builds a new `Editor` per buffer. We replace the
  `Viewport` with `Viewport::default()` in `open_buffer`.
- **`vertical` is generic over `CaretRows`.** Zed's snapshot builds in a
  test without a window. `DocumentLayout` does not: its `WrappedLine`s come
  from a `WindowTextSystem`, and gpui keeps `WrappedLine::layout`
  `pub(crate)`. A test fake is the second implementer (CLAUDE.md: one real,
  one test fake).
- **The hit test is `TextInput`'s.** Both hosts need it, and `last_bounds`
  is already there. Zed has one host.

## The module: `editor/viewport.rs`

`Viewport` owns where the editor's text sits under its scroll, and the scroll
waiting for a layout. It hides the text top at zero scroll, the request
policy, the first-paint frame, the top line of a reload and the reveal
geometry. It lives in `editor/`: only the editor scrolls. It needs no
`Window`, so its policy tests are plain `#[test]`s.

`scroll.rs` becomes `viewport.rs`; `scroll/tests.rs` becomes
`viewport/tests.rs`. `Reveal`, `center` and `MARGIN_LINES` move unchanged;
the free `reveal` becomes the private `minimal`, pairing with `Reveal::Minimal`.

### Types

```rust
/// A scroll decided before the geometry it needs; `render` applies it.
#[derive(Debug, PartialEq)]
pub(super) enum ScrollTo { Top(TopLine), Cursor(Reveal) }

/// The text at the viewport's top, to put back there once the new text is laid out.
#[derive(Debug, PartialEq)]
pub(super) struct TopLine { offset: ByteOffset, below: Pixels }

#[derive(Default)]
pub(super) struct Viewport {
    handle: ScrollHandle,
    /// Text top at zero scroll, as of the last paint.
    text_top: Option<Pixels>,
    pending: Option<ScrollTo>,
}

impl Viewport {
    /// For `track_scroll`, and tests reading the offset.
    pub(super) fn handle(&self) -> &ScrollHandle;
    /// From `paint`, with the painted (scrolled) text origin. True on the first
    /// paint while a scroll waits: the caller defers a notify for the next frame.
    pub(super) fn painted(&mut self, text_origin_y: Pixels) -> bool;
    /// A reveal replaces any request; a top line only fills an empty slot.
    pub(super) fn request(&mut self, to: ScrollTo);
    /// Scrolls now when the geometry is current, else requests.
    pub(super) fn reveal(&mut self, layout: Option<&DocumentLayout>, cursor: ByteOffset, how: Reveal);
    pub(super) fn take_pending(&mut self) -> Option<ScrollTo>;
    /// `layout` must describe the current text: the caller reshapes first.
    pub(super) fn apply(&mut self, to: ScrollTo, layout: Option<&DocumentLayout>, cursor: ByteOffset);
    /// A change from disk: requests the old top line, then maps any waiting request through `diff`.
    pub(super) fn reloaded(&mut self, diff: &Diff, layout: Option<&DocumentLayout>);
}
```

`TextInput` gains one method:

```rust
/// The layout and the point in its coordinates. `last_bounds` is already scrolled,
/// so no caller subtracts the offset.
pub(crate) fn hit_test(&self, position: Point<Pixels>) -> Option<(&DocumentLayout, Point<Pixels>)>;
```

`editor/motion.rs` gains the pure move:

```rust
/// What `vertical` reads from a layout.
pub(super) trait CaretRows {
    fn row_count(&self) -> usize;
    fn row_for_offset(&self, offset: ByteOffset) -> usize;
    fn holds_caret(&self, row: usize) -> bool;
    fn x_for_offset(&self, offset: ByteOffset) -> Pixels;
    fn offset_on_row(&self, row: usize, x: Pixels) -> ByteOffset;
}
impl CaretRows for DocumentLayout { /* forwards to the inherent methods */ }

/// The offset and goal one caret row up or down; `None` past the first or last row.
pub(super) fn vertical(rows: &impl CaretRows, cursor: ByteOffset, goal: Goal, direction: Vertical)
    -> Option<(ByteOffset, Goal)>;
```

### Invariants and ordering

- **G1.** Only `painted` writes `text_top`. Every y in the viewport's frame
  derives from `text_top` and the live offset, in one `viewport_top`.
  It is `pub(super)` for one reason: `EditorView`'s `#[cfg(test)]` helpers
  read it. No production caller outside `Viewport` does. Nothing derives it
  from `last_bounds`.
- **G2.** `reveal` scrolls at once only with a text top, a layout, a viewport
  taller than zero and no waiting request. Otherwise it requests
  `Cursor(how)`. A waiting request means the layout may predate the text.
- **G3.** `request`: `Cursor` replaces any request; `Top` fills only an
  empty slot.
- **G4.** `render` takes the request, reshapes, then applies it. The reshape
  stays in `EditorView`: it needs the `Window` and the markdown.
- **G5.** `open_buffer` sets `self.viewport = Viewport::default()` and calls
  `TextInput::rebind`. No field is cleared by hand.
- **G6.** The gates still reveal: `after_edit` reshapes, then reveals;
  `after_move` reshapes when the touched conceals differ, then reveals.
  Prepaint never reveals.
- **G7.** `vertical` never lands on a row where `holds_caret` is false. It
  returns `Goal::Column` of the incoming goal x, or of the caret's x when the
  goal is `None`. `move_vertically` applies it, or `DocStart`/`DocEnd` on
  `None`.
- **G8.** No new shape. `Viewport` reads the layout it is given; the frame
  pipeline is unchanged.

## What moves where

- **V1. `editor/viewport.rs`.** From `scroll.rs`: all of it. From
  `motion.rs`: `reveal_cursor`'s body, as `Viewport::reveal`. From
  `reload.rs`: `TopLine`, `ScrollTo::mapped`, `top_line`, `viewport_top`,
  `restore_top`. The `#[cfg(test)]` helpers `scroll_to_top`,
  `offset_at_top`, `scroll_y`, `caret_in_view` move to `editor/mod.rs`,
  with their names and signatures, so `shell/tests/reload.rs` is
  untouched. Their geometry is `#[cfg(test)]` `Viewport` methods that take
  the layout, so `viewport_top` stays private.
- **V2. `editor/mod.rs`.** `last_text_top`, `scroll_handle` and `scroll_to`
  become `viewport: Viewport`. `open_buffer` follows G5. `render` calls
  `restore_scroll` (take, reshape, apply) and
  `track_scroll(self.viewport.handle())`. `reveal_cursor` stays as a
  one-line wrapper: three callers.
- **V3. `editor/element.rs`.** `paint` calls `editor.viewport.painted(..)`
  and defers the notify when it returns true.
- **V4. `editor/reload.rs`.** Keeps `on_buffer_reloaded`: `map_through`,
  then `viewport.reloaded(diff, layout)`, then notify.
- **H1. `text_input.rs`.** `hit_test`. `character_index_for_point` uses it.
  `mouse.rs` drops `position_for_point`; `on_mouse_down`, `on_mouse_move`
  and `link_under` use `hit_test`.
- **M1. `editor/motion.rs`.** `CaretRows`, its impl for `DocumentLayout`,
  and `vertical`. `move_vertically` becomes: read the layout, call
  `vertical`, apply through `move_cursor`.

## Tests

The new interfaces are the test surface. Each line is one executable
criterion and names the bug it catches.

`editor/viewport/tests.rs`, plain `#[test]`, no window (`ScrollHandle::new`
needs no `App`):

- `a_reveal_before_the_first_paint_waits_for_it`: a default viewport,
  `reveal(None, .., Center)`; `take_pending` is `Cursor(Center)`, offset
  still 0. Fails if G2 drops the request.
- `a_reveal_replaces_a_waiting_top_line` and
  `a_top_line_never_replaces_a_waiting_reveal` (G3). Fail if the policy
  swaps or always replaces.
- `only_the_first_paint_with_a_waiting_scroll_asks_for_a_frame`: `painted`
  returns true once, then false; false on a first paint with nothing
  waiting. Fails on a redraw loop or a lost frame.
- `a_reload_maps_a_waiting_top_line_through_the_diff`: pending
  `Top` at 50, `reloaded` with `inkra_text::diff` inserting 10 bytes at 0
  and no layout; pending is `Top` at 60. `a_reload_keeps_a_waiting_reveal`:
  pending `Cursor` survives unchanged.
- `a_second_reload_drops_its_top_line_for_the_waiting_one`: pending `Top`
  at 50, a reload whose fresh top line (from the pre-reshape layout) is at
  20, the diff inserting 10 bytes at 0; pending is `Top` at 60, not 20 or
  30. `a_first_reload_maps_its_own_top_line`: empty slot, fresh top line at
  50, same diff; pending is `Top` at 60. Together they pin
  request-then-map: the composed path `on_buffer_reloaded` runs with a real
  layout. `DocumentLayout` cannot be built without a window, so `reloaded`
  computes the fresh top line from the layout, then hands it to a private
  `follow(diff, fresh: Option<TopLine>)` that requests and maps; the tests
  call `follow`. Computing the top line from a real layout stays covered by
  `shell/tests/reload.rs`.
- The seven reveal tests and the proptest, moved from `scroll/tests.rs`,
  bodies unchanged.

`editor/motion/tests.rs`, plain `#[test]` and `proptest`, over a `Grid`
fake: rows as `(start, caret_end)` byte ranges, 10 px per byte, a wrap
offset belonging to the next row as in `DocumentLayout`:

- `down_keeps_the_goal_across_a_short_row`: rows 30, 5, 30 bytes wide,
  goal at 200 px; two downs land at 20 bytes into row 2. Fails if the goal
  is re-read from the landing.
- `a_move_without_a_goal_starts_from_the_caret_x`.
- `down_steps_over_a_row_that_holds_no_caret`: the middle row shares its
  start with the next. Fails if the skip loop goes.
- `up_from_the_first_row_is_none` and `down_from_the_last_row_is_none`.
- Proptest `a_vertical_move_lands_on_a_caret_row_in_its_direction`: random
  grids with collapsed rows, any cursor, both directions; the landing row
  holds a caret and is strictly above (up) or below (down) the start.

`text_input/tests.rs`:

- `a_hit_test_while_scrolled_uses_the_painted_bounds`: on `EditorView`,
  scroll, draw, `hit_test` a point on line n; the offset is line n's. Fails
  if `hit_test` subtracts the offset again.

Kept green, bodies unchanged except paths: every test in `editor/tests/`
(`scroll.rs`, `wrap.rs`, `shapes.rs`, `swap.rs`), `shell/tests/reload.rs`,
`shell/tests/vault_search.rs`, `text_input/tests.rs`,
`editor/tests/protocol.rs`. The path changes are `view.scroll_handle` to
`view.viewport.handle()`, and `tests/scroll.rs:468`'s
`view.scroll_to = Some(..)` to `view.viewport.request(..)`, which gives the
same state on an empty slot. `shapes.rs::a_keystroke_shapes_the_note_once`
is G8's criterion.

Run `./scripts/test-names.sh` before step 1 and after each step, and diff.
Only `scroll::tests::*` changes path, to `viewport::tests::*`; the rest are
additions.

## Look

Render `editor-caret`, `editor-wrapped`, `editor-wrapped-selection`,
`editor-concealed`, `reload-scroll` and `vault-search-opened` before step 1
and after the last step. They must match byte for byte.

## Risks

- **The fake can drift from `DocumentLayout`.** It models the wrap rule by
  hand. The window tests in `tests/wrap.rs` stay as the integration check,
  `up_steps_over_a_row_with_no_caret_position` among them. If review
  rejects the trait, the fallback is testing `vertical` on a layout shaped
  in a headless window: no trait, no proptest over collapsed rows.
- **G4 is an ordering the type does not enforce.** `apply` could read a
  stale layout if a future caller skips the reshape.
  `a_reveal_waiting_through_a_reload_centres_by_the_new_text` fails if it
  does.
- **Borrows.** `self.viewport.reveal(self.input.last_layout().map(Rc::as_ref), ..)`
  borrows two disjoint fields; the wrapper keeps it in one place.
- **The roadmap item.** A `TextElement` that owns its scroll would take the
  `Viewport` into its prepaint: `text_top` and the deferred notify then go,
  and `take_pending` moves from `render` to prepaint. This refactor puts
  everything that move touches in one type, so it gets easier.

## Plan

Each step builds, passes `./scripts/check.sh`, diffs `test-names.sh`, and is
one commit.

1. **Vertical move, pure.** M1 with `motion/tests.rs`. The `wrap.rs` up and
   down tests pass unchanged.
2. **Hit test.** H1 and its test. The click tests in `tests/scroll.rs` and
   the protocol tests pass unchanged.
3. **Viewport owns the scroll.** Add `viewport.rs` from `scroll.rs`,
   `Viewport` with `handle`, `text_top`, `pending`, `painted`, `request`,
   `reveal`, `take_pending`, `apply`. V2, V3, G5. Move `scroll/tests.rs`,
   add the policy and first-paint tests. `tests/scroll.rs` path changes.
   G2's last clause, a reveal behind a waiting request, needs a painted
   viewport; `a_hit_picked_while_a_reload_waits_centres_by_the_new_text`
   already pins it.
4. **Reload through the viewport.** `TopLine`, `reloaded`, the top-line
   helpers into `viewport.rs` (V1, V4). Add the two reload tests.
   `shell/tests/reload.rs` passes untouched. Shots match.
5. **Close.** `.claude/rules/editor.md`: the `request_scroll` line names
   `Viewport::request`; add that pixel to offset from a window point goes
   through `TextInput::hit_test`, and that `open_buffer` resets the
   viewport by value. Add this spec to the CLAUDE.md docs list. In
   `docs/roadmap.md`, the scroll item notes that `Viewport` is what moves
   into the element. Append a one-line pointer, "superseded by
   `2026-09-29-editor-geometry-design.md`: `scroll.rs` is now
   `viewport.rs`, `scroll_to` is now `Viewport`, `request_scroll` is
   `Viewport::request`", to the `scroll_to` / `editor/scroll.rs` paragraph
   of `2026-09-28-vault-search-integration-design.md`, and to the
   `last_text_top` / `request_scroll` mentions of
   `2026-09-29-text-input-design.md`. `cargo mutants` does not apply: the diff touches only
   `inkra-app`.

## As built

Deviations from the plan above, each with its reason:

- **`reveal_cursor` and `restore_scroll` sit in `mod.rs`,** beside
  `reshape`, not in `motion.rs` and `viewport.rs`. Both are `EditorView`
  glue: they pass the layout and the cursor to `Viewport`, and
  `restore_scroll` reshapes. `reveal_cursor` is private to `editor`.
- **`TextInput::last_bounds` is `#[cfg(test)]`.** After H1, `hit_test` is
  its only production reader, so the accessor stays for tests only.
- **`mouse.rs` inlines the hit.** `position_for_point` is gone;
  `on_mouse_down` and `on_mouse_move` map `hit_test` through
  `offset_for_position` themselves, and `link_under` reads its point.
- **`ScrollTo::mapped` and `Viewport::top_line` are private** to
  `viewport.rs` once step 4 lands; only `reloaded` calls them.
- **The hit-test test scrolls with `scroll_to_top`,** the existing
  `#[cfg(test)]` helper, not with keystrokes: `text_input/tests.rs`'s editor
  window binds no keys.
- **Look.** `editor-caret`, `editor-wrapped`, `editor-wrapped-selection`,
  `editor-concealed`, `reload-scroll`, `vault-search-opened`, and also
  `editor-headings` and `editor-heading-revealed`, rendered at `4c4e055`
  and after step 4: byte for byte identical.
- **`test-names.sh`** shows 15 additions and no removal. The moved
  `scroll::tests::*` keep their leaf names under `viewport::tests`.
