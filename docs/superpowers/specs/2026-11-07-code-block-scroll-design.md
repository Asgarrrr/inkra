# Code blocks: a horizontal scroll per block

Status: **draft 2026-11-07, revised after red and blue teams**, for review. Builds on
`2026-11-07-code-blocks-design.md` (C1–C3, #162) and replaces its decision 5
("Lines wrap at the column").

## Why

A code line longer than the column wraps today. Wrapped code reads badly:
indentation breaks, and a long line looks like two. Code reads best on one
row per line, with the overflow reachable sideways. Prose keeps soft wrap.

## Decisions taken

1. **Each block scrolls on its own.** A block line is shaped with no wrap.
   The block's text moves sideways under its background; the page does not.
2. **A global setting, scroll by default.** `code_block_wrap` in
   `.inkra/settings.json`, default `false`. `true` brings back wrap at the
   column, as C1–C3 built it. No per-block toggle.
3. **Axis lock per gesture.** A trackpad gesture locks to its dominant axis,
   with gpui's `OngoingScroll` rules. A horizontal gesture over a block scrolls
   that block; a vertical one scrolls the page. At the block's limit the
   block keeps the gesture: the page never takes it over.
4. **Vertical motion keeps the screen x.** Up and Down into a scrolled block
   land under the same screen column, with no scroll, when that line reaches
   the column. A shorter line puts the caret at its end; if that end is
   hidden, the reveal (D5) scrolls the block back to show it. Zed does the
   same.
5. **A thin bar shows the position**, inside the background's bottom edge,
   while the block scrolls and while the pointer is over it. It does not
   drag.
6. **The offset is per tab, in memory.** A parked tab keeps it; the session
   file does not store it. A restart shows every block at its left edge.

## D1. Where the offset lives

**One state, shared by every layout.** `EditorView` owns a `BlockScrolls`
handle. Every `DocumentLayout` it shapes holds a clone, and reads it live.
This is gpui's `ScrollHandle` pattern (`div.rs:4243`): an `Rc<RefCell<_>>`
shared between the view that writes and the element that reads.

```rust
#[derive(Clone, Default)]
pub(crate) struct BlockScrolls(Rc<RefCell<Vec<BlockScroll>>>);

struct BlockScroll {
    /// The block's first byte, `Bias::Right`: a line typed at the block's
    /// start keeps the anchor on the fence.
    anchor: Anchor,
    /// `anchor` resolved at the last `refresh`; sorted by it.
    start: ByteOffset,
    x: Pixels,
}
```

One entry per block scrolled at least once, created on the first scroll and
kept at `x = 0`. `Buffer` never reuses a forgotten anchor's slot, and
`shift_anchors` walks every slot on each edit (`anchors.rs:37-58`). One anchor
per block, not one per scroll from zero, keeps that growth to the number of
blocks scrolled in a view's life. The layout reads `x` by its block's `start`
and clamps it to the block's `max` at read time.

Why not a value in the layout: the layout is an immutable `Rc`, cached by its
inputs (`is_shaped_from`). A scroll in `LayoutInputs` would reshape on every
wheel tick. A per-layout `Cell` written in prepaint fails too. `reshape`
(`editor/mod.rs:212`) builds a new layout outside any draw, from
`after_edit` and `after_move`, and `request_layout` measures several per
frame. Each would read zero until the next prepaint. With one shared handle,
every layout instance answers with the same offsets, whenever it is asked.

**Writers.** Only `EditorView`: the wheel listener (D6), the reveal (D5),
and `refresh`. The reveal can run in prepaint, after `Viewport::resolve`
(D5). No borrow of the `RefCell` is held across a call, so a read and a write
never overlap.

**`refresh` keeps it in sync.** `refresh` (`mod.rs:148`) is reached before
every reshape, in `render`, in `on_buffer_changed` and in `open_buffer`. It
parses the note once per buffer version. The sync runs on every call, before
the version check: an undo can restore a version with an anchor moved
(`editor/parked.rs`, `peek`), and the sync costs O(entries). It re-resolves
each anchor and drops the entries whose `start` is not a block
start in `markdown.code_blocks`, forgetting their anchors. Then it sorts the
rest by `start`. This is Zed's `retain_code_block_scroll_handles`
(`markdown/src/markdown.rs:3282`). A fence typed above re-pairs the blocks
below, so their entries drop and they show at their left edge. Retyping the
fence does not bring the offset back. Accepted.

**The anchors' lifecycle.** It follows the pin's (`drop_pin`, `mod.rs:187`):

- `ViewState` gains `scrolls: Vec<(ByteOffset, Pixels)>`, plain data, as
  `TopLine` is for `ParkedTop`. `capture` fills it, so history and tab
  switches carry the offsets.
- `ParkedView` anchors them on park, and resolves and forgets them on
  unpark, as it does the selection and the top line.
- `open_buffer` and `restore` forget every entry's anchor in the old buffer
  before the rebind, as `drop_pin` does, then clear the handle and anchor the
  view's `scrolls` in the new buffer. The next `refresh` prunes any that is
  no longer a block start.
- `SavedView` drops `scrolls`: the session file never stores them.
- `EditorView`'s `Drop` debug-asserts the handle is empty, as it does for the
  pin (`mod.rs:352`).

**The clamp.** `max = (widest − column).max(0)`, computed in `Blocks::close`.
`widest` is already the block's content width (`blocks.rs:119`), and the
background's right edge is `column + CODE_BLOCK_OUTSET`. Off the editor
(`column = None`, the name field) `max` is zero, so nothing scrolls. A write
clamps to the current layout's `max` and removes the entry at zero. A read
clamps again, since a resize or an edit can shrink `max`.

## D2. Shaping

`shape.rs` passes `wrap: None` to a line that `line_holder` places in a block,
unless `code_block_wrap` is set. `LayoutInputs` gains `code_block_wrap: bool`
and `scrolls: &BlockScrolls`. `is_shaped_from` compares the flag, never the
scrolls. The shape cache keeps working: `shaped_from` already compares
`wrap`. `NameField` passes `false` and an empty handle.

Each block line is then one row. The document is shorter than with wrap. A
reload's `TopLine` restore still holds: it maps an offset, not a row index.

A revealed info string longer than the column no longer adds a row. It widens
the block's `widest` instead. This replaces the code blocks spec's D4
exception.

## D3. Geometry

**Two functions, split by construction.** `x_on_row` becomes:

- `content_x(line, within, offset)`: today's body, unshifted, still an
  associated function;
- `x_on_row(&self, line, within, offset)`: `content_x` minus the line's block
  offset, for the text branch only. It needs `self` for `code_blocks` and the
  handle; every caller but `Blocks::line` already has it.

`Blocks::line` reads the background's left edge with `content_x`, and so does
the reveal (D5). Neither can read a scrolled value.

`LineLayout` gains `block: Option<usize>`, the index in `code_blocks` of the
block that holds it. `BlockLayout` gains `start` and `max`. `shape.rs` knows
the index before building the `LineLayout`: `line_holder` runs first, and
`Blocks` counts the blocks it has opened. `offset_on_row` adds the same offset back before
`closest_index_for_x`. The prefix branch is unchanged: a quote's `>` and its
gutter never move. So on a scrolled quote block, `x_on_row` jumps backwards
at the prefix's end. A test pins this.

Through those two functions, the shift reaches `x_for_offset`,
`caret_bounds`, `range_bounds` (IME), `selection_quads`, `row_end_x`, the
newline stub, `link_under`, `offset_for_position` (click, drag, IME) and
`CaretRows`. `Goal::Column` is already a screen x, so decision 4 needs no
code.

**A click on hidden text.** A click left of the background on a scrolled
block line maps to a hidden glyph. The caret lands there, and the reveal (D5)
scrolls it into view. A drag past either edge extends the selection the same
way, one step per mouse move, as Zed's `autoscroll_code_block` does. A drag
held still past the edge stops; no timer. Accepted.

## D4. Paint

The background, the label, the quote bars and the prefix do not move.

- **Text.** `DocumentLayout::paint` paints a block line at `x − scroll`,
  inside `window.with_content_mask(background)`. The mask is the background,
  rounded corners ignored.
- **Selection.** `selection_quads` clips each quad on a block row to the
  block's background. A quad left wholly outside it on the left, as a
  selected blank line's newline stub, is kept as a stub at the background's
  left edge, so a selected blank line still shows. The layout knows the blocks;
  `Quads::new` stays generic, and `NameField` is unaffected.
- **Caret.** `DocumentLayout::caret_visible(offset)` is false when the caret
  on a block row lies outside the background. `Quads::new` then paints no
  caret quad. `caret_bounds` is unchanged, so the IME still gets the true
  position.
- **The bar (S3).** It is a rounded quad 3 px high, its bottom 1 px above
  the background's bottom. A 16 px line centred in a 28 px row keeps its
  descenders above that band; the scene checks it, a revealed closing fence
  included. Its width
  is `visible × visible / (visible + max)`, at `x × visible / (visible + max)`.
  It uses `text_placeholder`. It is painted after the text and before the
  caret.

## D5. Reveal

`Viewport::reveal` keeps its vertical job. `EditorView::reveal_cursor` gains a
horizontal pass for a caret on a block line. The pass runs after the vertical
one and never moves y.

- The caret's content x is its `content_x`.
- `DocumentLayout::block_span(offset)` gives the block's `start`, its text
  left, its visible width (`column − text left`) and its `max`.
- If the caret is outside `x .. x + visible`, keep
  `CODE_BLOCK_SCROLL_MARGIN` (16 px) to its right. Scroll the minimum amount,
  as Zed's `autoscroll_horizontally` does
  (`editor/src/scroll/autoscroll.rs:372-491`).
- `Reveal::Center` (a search hit) is minimal on x too.

The pass writes `BlockScrolls` directly. Every layout reads it at once, so the
caret and the quads built in the same prepaint already show the new x.

`reveal_cursor` (`mod.rs:295`) takes `cx` to reach the buffer, for a new
entry's anchor. Its callers, `after_edit`, `after_move` and `select_range`
(`motion.rs:31`), pass it.

**A stale layout.** When the vertical reveal is pending (`ScrollTo::Cursor`),
`Viewport::resolve` applies it in prepaint. Prepaint then runs the horizontal
pass against the measured layout, through `editor.update`, before
`Quads::new`. The caret and the quads of that frame already show the new x.

## D6. Wheel input

**The editor div** gains `.restrict_scroll_to_axis()`. Today gpui remaps a
pure horizontal delta to y on a vertical-only scroller (`div.rs:3357-3363`).
A sideways swipe anywhere in a note therefore scrolls the page; with the flag
it does nothing outside a block. Side effect: the page's vertical gestures
lock to their axis too, so a diagonal swipe no longer scrolls y with a little
drift. Accepted; it is what the flag is for.

**One gesture filter per editor.** `EditorView` owns it (see *Deviations in the implementation*: `Gesture`). The listener closure
is rebuilt every paint, and `TextElement` has no element state (`id()` is
`None`), so state held in the closure would reset on every event and never
lock.

**`TextElement`** inserts one hitbox for the text bounds and one per visible
block with `max > 0`, in prepaint, from `text.origin` after its y shift
(`element.rs:124`). The listener tests them with `should_handle_scroll`, as
the div does (`div.rs:3335`). `is_hovered` is false after a keystroke until
the mouse moves (`window.rs:778`), so a swipe after typing would miss the
block. In paint it registers one
`ScrollWheelEvent` listener, bubble phase, over the text hitbox:

1. Gate on `delta.precise()`, as the div does (`div.rs:3339`). A mouse
   wheel's `Lines` delta is never horizontal on macOS without shift, and
   shift+wheel already arrives as pixels.
2. Feed every precise event over the text to the editor's gesture filter,
   so the lock is per gesture, wherever it started. A vertical swipe that
   crosses a block stays vertical.
3. Consume only a delta locked horizontal (`y == 0`, `x != 0`) over a block's
   hitbox. Update that block's entry, clamped to `0..=max`. Call
   `stop_propagation` and `cx.notify` on the editor: the div's own notify is
   skipped. At the limit the event is still consumed, so the gesture does not
   fall through to the page.
4. Anything else returns: a vertical or an unlocked diagonal delta goes to the
   div.

gpui runs bubble-phase mouse listeners in reverse registration order
(`window.rs:5780`). The div registers its scroll listener before it paints its
children (`div.rs:2589`, then `2605`), so the element sees the event first. A
test pins it. `stop_propagation` also skips every listener registered before
the element. Only the div's sits there today.

macOS momentum arrives as decaying `Moved` events after `Ended`
(`gpui_macos/events.rs:258-285`). The filter starts a new gesture on it;
sideways momentum locks sideways, any other belongs to the page. There is no rubber band, as in the rest of
gpui.

## D7. The setting

`settings.json` is parsed in one place, `settings()` in `theme/store.rs`.
`load` returns `Resolved { choice, theme, code_block_wrap, notices }`; a
missing or non-boolean value is `false`. The existing reload path
(`concerns`, the watcher) re-reads it on an edit. `write_choice` already
preserves the other keys (`store.rs:244`).

The value reaches the layout through a sibling global, `CodeBlockWrap(bool)`
in `theme.rs`, set where `adopt_theme` (`shell/themes.rs:15`) installs the
theme. `GlobalTheme::set` returns early on an unchanged theme, so a change
of `CodeBlockWrap` calls `refresh_windows` itself. It is set outside the
branch that skips `install` while the theme picker is open
(`shell/themes.rs:20-23`). `bootstrap.rs:126-128` installs it from the loaded
`Resolved` beside the theme, so the first window shapes with the right wrap.
`Resolved::default()` carries `false`. The layout reads it with
`try_global`, so tests and the shot harness need not set it. There is no palette command yet
(Deferred).

## Edge cases

| Case | Behaviour |
|---|---|
| A block narrower than the column | `max = 0`: no hitbox, no entry, no bar |
| A block in a quote | The gutter and bars stay; the text scrolls under the background, which starts past the gutter |
| A block in a list | The indent spaces are text, left of the background: they scroll and are clipped. They are whitespace |
| Typing at the end of a long line | The reveal keeps the caret 16 px inside the right edge |
| An edit shortens the widest line | The read clamps to the new `max`; the next write stores it |
| A fence typed above | Re-paired blocks lose their entries and show at the left edge |
| The column narrows (pane resize) | `max` grows; the entry is kept |
| Reveal on caret entry | The fences show; a long info string widens `max` |
| Down onto a short line, end hidden | The block scrolls back to show the caret (decision 4) |
| `code_block_wrap: true` | C1–C3 behaviour: wrap, `max = 0`, no bar, no hitbox |
| A note without blocks | One text hitbox, no entry, nothing else per frame |

## Citations

File:line references into `vendor/zed-reference` are at the pinned commit and
drift by a few lines when it moves; re-pin them then.

## Deviations from Zed

- **Painted, not a `div`.** Zed's markdown preview wraps each block in an
  `overflow_x_scroll` div with a `ScrollHandle` keyed by the block's source
  offset (`markdown/src/markdown.rs:505, 2771-2840`). Our editor paints rows
  as quads (code blocks spec), so the offset is ours. It is keyed by an
  `Anchor`, not an offset, so an edit above the block keeps it.
- **x in pixels, not columns.** Zed's editor stores x in em advances
  (`editor/src/scroll.rs:35`). A block mixes a body-font indent with Menlo
  text, so a column is not one width.
- **The goal stays a screen x.** Zed keeps the goal in columns, so after a
  short line scrolls the block back, the next long line scrolls it right
  again. We keep the user's choice (decision 4): after the scroll back, the
  next long line lands under the same screen x.
- **No drag on the bar.** Zed's editor scrollbar drags. Deferred.

## Deviations in the implementation

- **`max` adds `CODE_BLOCK_SCROLL_MARGIN` (16 px).** At `widest − column`, the
  last glyph would end on the background's right edge.
- **An entry set to zero stays.** D1 says both "kept at `x = 0`" and "removes
  the entry at zero". Kept wins: the anchor is reused when the block scrolls
  again.
- **The left clip starts once `x > 0`.** Unscrolled, a list indent, an
  under-indented line and a column-0 caret may lie left of the background;
  a clip would hide them.
- **Only a block with `max > 0` clips, masks or takes the wheel.** A block
  that fits has nothing to hide.
- **A `Lines` delta scrolls a block.** Only the gesture filter is gated on
  `precise()`, as the div gates it (`div.rs:3339`). A `Lines` delta with no
  y is consumed over a block, unfiltered.
- **The consume rule is "filtered y is zero, raw delta is not".** An all-zero
  raw delta is never consumed. A sideways tick in a vertical gesture filters
  to zero; over a block it is consumed and moves nothing.
- **The sync runs after the reparse, on every `refresh`.** D1 puts it before
  the version check. It needs the current `code_blocks` to tell a block start,
  and a table already in sync is cheap to sync again. `on_wheel` calls
  `refresh` first and refuses a start that is no block start: its hitbox may
  come from a frame drawn before an edit.
- **Release forgets the anchors in `cx.on_release`, not in `Drop`.** `Drop`
  has no buffer access; it only debug-asserts the table is empty. gpui's test
  harness drops entities without releasing them, so a test that leaves a
  block scrolled removes its window at the end (`close`), as the split tests
  do for a pin.
- **"Gone at zero" reads 0 and leaves `ViewState`.** An entry at zero keeps
  its anchor until the view leaves the buffer; `entries` omits it.
- **The mask is horizontal only.** It keeps the current top and bottom: every
  block row lies inside the background, so its top and bottom cut nothing.
- **The bar's sizes and colour are D4's.** 3 px high, 1 px above the background's bottom,
  radius 1.5 px, `text_placeholder`; no difference. They are `CODE_BLOCK_BAR_*` in `theme.rs`.
- **A restored offset stamps as a scroll.** `anchor_all` writes through `BlockScrolls::set`, so
  a tab switch, a back or a rebind onto a scrolled block shows its bar for `BAR_LINGER`, as a
  wheel and a reveal do. Each replaces `bar_expiry`.
- **`Gesture`, not `OngoingScroll`.** `EditorView` owns a `Gesture` that keeps
  `OngoingScroll::filter_at`'s 28 ms gap, and its 1.9 ratio and 6 px floor to
  unlock. gpui's `filter` reads the wall clock and `filter_at` is private, so
  the axis-lock tests depended on test speed. `Gesture` reads the executor's
  clock; tests cross the gap with `advance_clock(GESTURE_GAP)`. It keeps only a
  sideways lock: a gesture that starts vertical is the page's from its first
  event (next entry), so a vertical lock would change nothing.
- **One gesture has one taker.** The first event of a gesture that moves
  something picks the page or the block; the other refuses the rest of the
  gesture. Without it, a vertical gesture the filter unlocked could move a block
  with a later `y == 0` event.
- **A mouse wheel tick is a gesture of its own.** An imprecise event resets
  `Gesture` and is not filtered, as the div leaves it unfiltered.
- **The wheel listener also takes a block's padding.** It gates on any block
  hitbox, then on the text hitbox: the padding lies outside the text.
- **The linger is `BAR_LINGER`, 1 s.** A bar shows while `now - scrolled_at < BAR_LINGER`, with
  `now` from the executor's clock, so tests move it with `advance_clock`.

## Tests

`app`, `#[gpui::test]`, against `MemFs`:

- With the default setting, a long block line is one row. With
  `code_block_wrap: true` it wraps, as before.
- A horizontal pixel delta over a block moves only that block. `x_for_offset`
  of a block offset moves by `−x`; a prose offset and a second block do not.
- A vertical delta over a block scrolls the page and leaves the block. A
  vertical gesture started over prose that crosses a block stays vertical.
- The clamp: past `max` it stops at `max`, under zero at zero; the entry is
  gone at zero. The event is consumed at the limit.
- A keystroke in a scrolled block: the reshaped layout, before any draw,
  still answers `offset_for_position` with the scroll applied.
- An edit above the block keeps its offset. A fence typed above drops it.
- `offset_for_position` inverts `x_for_offset` on a scrolled line, the quote
  prefix's jump included.
- Down from prose into a scrolled block keeps the screen x. Down onto a short
  line with its end hidden scrolls the block back.
- Typing past the right edge reveals the caret with the margin. Home reveals
  column 0. A reveal resolved in prepaint applies its x in the same frame.
- A caret scrolled out of the background paints no caret quad; its
  `caret_bounds` is unchanged. A selection quad stops at the background.
- A parked tab comes back with its offsets, and so does Back. `open_buffer`
  and `restore` forget the old buffer's anchors. Scrolling a block to zero
  and back reuses its anchor.
- A selected blank line in a scrolled block shows its stub.
- A swipe right after a keystroke, the mouse unmoved, scrolls the block.
- The element's wheel listener runs before the div's (D6).
- `settings.json` sets and clears `code_block_wrap` live, with the theme
  unchanged.
- A wheel tick reshapes nothing: `line_shape_count` is unchanged across it.
- Rename `revealing_never_scrolls_horizontally` (`tests/scroll.rs:284`) to
  say the page never scrolls horizontally. The assertion stays.

Every new key path gets a `simulate_keystrokes` test, as the repo requires.

Scenes: `code-blocks` gains a scrolled block, and a block in a quote. A
`code-blocks-wrap` scene shows the setting on. Look at each after every slice.

`cargo mutants` on `inkra-markdown` is unaffected; this spec changes `app`
only.

**Measure.** The keystroke median on a note dense with blocks, interleaved
against base. Per frame, the cost added is one binary search per block line
painted, one mask per visible block, and one hitbox per visible block.
`paint_line` still walks every glyph of a long line (gpui culls at
`line.rs:731`); that is no worse than the same glyphs wrapped.

## Slices

Each slice is vertical: data, behaviour, test, scene.

1. **S1, scroll.** No-wrap shaping for block lines (the setting's default
   hard-coded). `BlockScrolls`, `refresh`'s sync, the anchors' lifecycle and
   `ViewState.scrolls`, the
   `content_x`/`x_on_row` split, `LineLayout.block`, `BlockLayout.max`. The
   mask, the selection clip, `caret_visible`. The wheel listener with the
   editor's gesture filter, and `restrict_scroll_to_axis` on the div. A block
   scrolls with the trackpad, and clicks land right.
2. **S2, reveal.** `block_span`, the horizontal pass in `reveal_cursor` and
   its `cx`, the margin, the pass after a prepaint reveal, and the drag past
   the edge it yields. Typing and arrows keep the caret visible.
3. **S3, bar and setting.** The bar: shown while a scroll is less than 1 s
   old and while the pointer is over the block's hitbox. A scroll schedules
   a notify at its expiry (`app` reads the clock). A mouse-move listener
   notifies when the hovered block changes. This is the per-block hover state
   the copy button needs too. Then `code_block_wrap` in `settings.json`,
   `CodeBlockWrap`, and the `code-blocks-wrap` scene.

## Deferred

| Item | Trigger |
|---|---|
| A palette command to toggle wrap | Asked for, or the setting is changed often |
| Dragging the bar | A block too wide to reach with the trackpad |
| Offsets in the session file | Asked for after a restart |
| A drag held past the edge keeps scrolling | Asked for |
| A wrapped block line hangs at its indent | `code_block_wrap: true` reads badly |
| Momentum or rubber band beyond what macOS sends | gpui gains them |
| A copy button on hover | S3's hover state makes it cheap; its own slice |
