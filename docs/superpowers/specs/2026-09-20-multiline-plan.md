# Multi-line editing — implementation plan

Status: **revised after red and blue review**. Supersedes the ordered sketch in
`docs/gpui-notes.md` "Path to multi-line", which it makes concrete.

What review changed, for the record:

- D3 reversed. Scrolling is in, because the cost estimate that excluded it was
  wrong — see D3.
- D4 reshaped. The goal column moves into `Selection` as an opaque scalar, which
  turns two invariants review found leaking into compiler errors.
- Slices reordered. The original order did not compile: binding `Enter` before
  per-line rendering puts a `\n` into `shape_line`, whose `debug_assert`
  (`text_system.rs:372`) fires under `cargo test`.
- D5 gained clamping everywhere, and `LineLayout::top` was cut as derivable.
- Per-line `TextRun`s must be rebased line-relative — the one finding that would
  have shipped as mis-shaped text rather than a panic.

The editor renders and edits exactly one line today. `shape_line` asserts the
text holds no `\n` (`text_system.rs:373`), the element requests a height of one
`line_height` (`element.rs:53`), and `last_layout` is a single `ShapedLine`
(`mod.rs:29`). Every geometry call — cursor quad, selection quad,
`bounds_for_range`, mouse hit-testing — is `x_for_index` against that one line.

## Scope

In:

1. A logical line index on `Buffer`.
2. `Enter` inserts a newline.
3. `Home`/`End` become line-relative; `cmd-up`/`cmd-down` keep the document ends.
4. Per-logical-line shaping and painting.
5. Multi-line cursor, selection, and mouse hit-testing.
6. `Up`/`Down` with a goal column.
7. `bounds_for_range` and `character_index_for_point` made line-aware.

Out, deliberately, each with the trigger that brings it back:

- **Soft wrap.** `shape_line` per logical line, `wrap_width: None`. Returns when
  a real note is measured to need it (`gpui-notes.md` step 7).
- **`uniform_list` / virtualization.** Every line is shaped every frame. Returns
  when a profile on a real note shows the per-frame `SharedString` allocations
  or the shaping loop cost anything.
- **CRLF normalization.** Nothing can put a `\r` in the buffer today: there is
  no file I/O and no paste action. Returns with whichever lands first.
- **Multi-cursor.** `Selection` is one range by construction.

Open decision: **scrolling** — see D3.

## Decisions

### D1 — the line index scans, it does not cache

`Buffer` gains pure scanning queries over its `String`. No memoized
`Vec<ByteOffset>` of line starts.

A cache here is an invariant that every one of `replace`, `apply`, `undo` and
`redo` must maintain, and that nothing in the type system enforces. The cost of
getting it wrong is a silently wrong cursor. The cost of not having it is a
linear scan on a note-sized file.

Complexity, stated so a profile can refute it: `line_for_offset` and
`line_start` are O(n) but are called a small constant number of times per frame
(cursor, selection ends). The element iterates the whole document once per
frame through `lines()`, which is a single `split('\n')` pass carrying a running
offset — O(n) total, not O(n·lines).

API:

```rust
impl Buffer {
    pub fn line_count(&self) -> usize;
    pub fn line_for_offset(&self, offset: ByteOffset) -> usize;
    pub fn line_range(&self, line: usize) -> Range<ByteOffset>;   // excludes the '\n'
    pub fn lines(&self) -> impl Iterator<Item = (ByteOffset, &str)>;
}
```

`line_range` excludes the trailing `\n`: the newline is a separator, not a
member of either line. A cursor may sit at `line_range(i).end` and at
`line_range(i+1).start`, which are different offsets — that is correct, and it is
what makes `End` then `Right` land on the next line.

Belongs in `text`, not `editor`: pure `&str` scanning, no layout input. The
architecture rule in `CLAUDE.md` puts it there, and `markdown` (block
boundaries) and `vault` (backlink ranges) are the second and third consumers
already named in the design doc.

### D2 — `shape_line` per logical line, not `shape_text` on the document

`shape_text` would work — it splits on `\n` internally
(`text_system.rs:504`) — but it returns `WrappedLine`, whose geometry API is
`position_for_index(index, line_height) -> Option<Point>` rather than the
`x_for_index`/`index_for_x`/`closest_index_for_x` the existing code is built on.
Adopting it means paying for the wrap-aware API while passing `wrap_width: None`.

Shaping per logical line also keeps the frame-to-frame layout cache useful. The
cache key embeds the whole shaped string (`line_layout.rs:606-613`), so one
document-wide call means one keystroke misses for everything; per line, only the
edited line's key changes and every other line hits `previous_frame`
(`line_layout.rs:493-500`).

### D3 — scrolling is in, on the existing `div`

The draft deferred it, estimating "scroll offset in every hit-test plus
scroll-to-reveal-cursor". That estimate was wrong, and blue review refuted it
from the source.

`Interactivity::prepaint` wraps its children in
`window.with_element_offset(scroll_offset, ...)` (`elements/div.rs:1413`). The
offset is applied to the child's coordinate space *before* `TextElement` sees
its `bounds`. So `element.rs` and `input.rs` need no scroll awareness at all:
`last_bounds` already comes back scrolled, and every existing
`point - bounds.origin` subtraction stays correct. Mouse-wheel handling is also
automatic — `paint_scroll_listener` registers itself whenever `overflow.y` is
`Scroll` (`div.rs:2403-2411`).

The whole change is two builder calls on the `div` `EditorView::render` already
has, plus a `ScrollHandle` field:
`.overflow_y_scroll()` (`div.rs:1062`) and `.track_scroll(&handle)`
(`div.rs:1077`).

Still out: **scroll-to-reveal-cursor**. It needs the cursor's y, so it depends
on `DocumentLayout` existing, and it is `ScrollHandle::set_offset`
(`div.rs:3211`) against `offset()`/`bounds()` — a self-contained follow-up.
Trigger to do it: the first time typing past the bottom edge loses the caret.

### D4 — the goal column lives in `Selection`, as an opaque scalar

The draft kept it as `EditorView::goal_x: Option<Pixels>`, cleared by
`move_cursor` and restored on the line after. Review broke that twice:

- `edit_buffer` is a different gate. Typing between two vertical moves left a
  stale goal, because only `move_cursor` cleared it.
- The sketch restored the goal unconditionally, including on the first/last
  line where the prose two paragraphs above said it must be cleared.

Both are the same failure: an invariant held by adjacency and a comment. The fix
is to make the compiler ask.

```rust
// editor/src/selection.rs — an opaque scalar. `editor` stores it; only `app`,
// which has the font metrics, knows it is a pixel x.
pub enum Goal { None, Column(f32) }
```

`Selection` carries a `Goal`, and `set_head`/`collapse_to` **take one**. There is
no way to move the head without stating what happens to the goal, so
`EditorState::edit` clears it (like it already clears `marked_range`,
`state.rs:87`) and every horizontal movement clears it, because both had to
choose.

`EditorState` then exposes exactly two movement entry points, mirroring
`EditorView`'s two gates:

- `move_to(buffer, offset, extend)` — clears the goal. Every existing caller.
- `move_vertically(buffer, offset, extend, goal)` — sets it. One caller.

Storing an `f32` in a gpui-free crate does not violate the architecture rule:
`editor` already stores `ByteOffset`, a number it does not interpret either.
What the rule forbids is *resolving* the goal, which still happens in `app`.
This is the shape Zed uses (`SelectionGoal` inside `Selection`,
`text/src/selection.rs:6-23`).

Semantics, from Zed (`movement.rs:127-132`): captured lazily on the first
vertical move, reused verbatim afterwards, and a short intervening line clamps
the resolved offset without touching the stored goal.

First and last line (`movement.rs:139-145`, `:174-180`): up on line 0 goes to
offset 0, down on the last line goes to the buffer end, and both go through
`move_to`, so both clear the goal by construction rather than by remembering to.

### D5 — a `DocumentLayout` owns every pixel↔offset conversion

Four call sites need it: the cursor quad and selection quads (`element.rs`),
`bounds_for_range` and `character_index_for_point` (`input.rs`), mouse
hit-testing and vertical movement (`mod.rs`). Duplicating the line search four
ways is how they drift.

```rust
// app/src/editor/element.rs — holds ShapedLine, so it is an `app` type.
struct LineLayout {
    shaped: ShapedLine,
    range: Range<ByteOffset>,   // excludes the '\n'
}

pub(super) struct DocumentLayout {
    lines: Vec<LineLayout>,     // never empty: an empty buffer is one empty line
    line_height: Pixels,
}

impl DocumentLayout {
    fn line_for_offset(&self, offset: ByteOffset) -> usize;
    fn line_for_y(&self, y: Pixels) -> usize;
    fn top_for_line(&self, line: usize) -> Pixels;
    fn x_for_offset(&self, offset: ByteOffset) -> Pixels;
    fn position_for_offset(&self, offset: ByteOffset) -> Point<Pixels>;
    fn offset_for_position(&self, position: Point<Pixels>) -> ByteOffset;
}
```

`LineLayout::top` is cut: with soft wrap out of scope every row is
`line_height` tall, so `top_for_line(i)` is `line_height * i`. Storing it would
be a second copy of a derivable fact — the thing D1 refuses for line starts.
Markdown live preview breaks the uniform-height assumption and brings a real
per-line `top` back; that is the right time to add it.

**Every index is clamped.** Review found two panics here, both from an unclamped
index into `lines`:

- A click below the last line. With scrolling and `size_full()`, the mouse
  listeners sit on the outer `div` (`mod.rs:162-165`), not on `TextElement`, so
  a click in the empty space under a short document still arrives with a `y`
  past the last row.
- A stale layout after a line-count-changing edit. Press `Enter`, and until the
  next paint `lines` has one fewer entry than the buffer has lines. This is
  strictly worse than the single-line staleness `gpui-notes.md` documents: that
  one could only return a stale *position*, this one an invalid *index*.

So `line_for_y` and `line_for_offset` clamp into `0..lines.len()`, and
`lines` is never empty. The worst case becomes a wrong caret for one frame,
which is the contract the file already documents — not a crash.

`offset_for_position` stays unclipped in the *byte* sense, like today's
`position_for_x`; both callers hand the result to `EditorState`, which clips.

`last_layout: Option<ShapedLine>` on `EditorView` becomes
`Option<DocumentLayout>`; `last_bounds` stays.

### D7 — per-line `TextRun`s are line-relative

`shape_line` is called per line with that line's own text, so its runs index
from 0 *within the line*. Today's code builds them from absolute buffer offsets
(`element.rs:85-88`), which is correct only because line 0 starts at 0.

Reused as-is on any later line, the run lengths stop summing to the line's
length, and `TextSystem::layout_line` (`text_system.rs:556`) hands those lengths
straight to the platform shaper. That mis-shapes text rather than panicking,
which is why it gets a decision of its own: intersect the marked range with each
line's range, then subtract `line.range.start` before building runs.

### D6 — `Enter` is an action, and it does not double-insert

A new `Newline` action bound to `enter`, handled through `edit_buffer`.

The risk raised in review is a double insert: `enter` is the only binding whose
`Keystroke::key_char` is `Some` on macOS (`platform/mac/events.rs:328`), so
unlike the arrows it is not routed through `doCommandBySelector:`. Checked in
the source rather than assumed: for a key with a `key_char` and no
control/function modifier, `handle_key_event` calls `run_callback` **first** and
returns immediately when the action reports it handled the key
(`platform/mac/window.rs:1735-1739`). The input handler is only reached when
nothing handled it, and the key-repeat path that calls `replace_text_in_range`
directly (`:1741-1750`) sits behind the same check. One press, one newline.

Composition is the one exception, and it is the behavior we want: while
`marked_text_range` is `Some`, Enter goes to the input handler first (`:1709`)
so it commits the composition instead of splitting the line.

The automated test — `simulate_keystrokes("enter")` leaves exactly `"\n"` —
proves the keymap and `actions!` wiring. It does *not* exercise
`handle_key_event`: `TestAppContext::dispatch_keystroke` calls
`window.dispatch_keystroke` directly and never enters the macOS platform layer.
So Enter joins font-kit on the short list of things `check.sh` cannot see —
press and hold it in the running app.

The editor-level primitive is `EditorState::insert(buffer, now, text)` —
replace the selection with `text`. Reusable by paste later, and it is the one
thing `EditorState` was missing.

## Implementation, as vertical slices

Each slice touches data, behavior, and a test, and each leaves `check.sh` green.

The draft's order did not: it bound `Enter` in slice 2 but only taught the
element to render lines in slice 3. In between, one `enter` in any test puts a
`\n` into `shape_line`, whose `debug_assert!` (`text_system.rs:372`) fires under
`cargo test`. Rendering therefore comes before anything that can create a
newline. Multi-line text reaches slice 2 through `Buffer::new("a\nb")`, which is
enough to verify it on its own.

### Slice 1 — the line index

- `crates/text/src/buffer.rs`: `line_count`, `line_for_offset`, `line_range`,
  `lines`.
- `#[test]`: empty buffer is one line; a trailing `\n` creates a last empty
  line; `line_range` excludes the newline; a multi-byte char does not split a
  line; `line_for_offset` at a line's exact start and exact end.
- `crates/text/tests/buffer_proptest.rs`: over random text and edit sequences —
  (a) `line_for_offset(line_range(i).start) == i` for every `i`; (b)
  `line_range(line_for_offset(o))` contains `o` for every valid `o`; (c)
  `lines()` yields `line_count()` items whose offsets match `line_range`;
  (d) concatenating `lines()` with `\n` reproduces `text()`.

### Slice 2 — every pixel becomes line-aware

The whole geometric change, in one slice because `last_layout` changes type and
all three files that read it must move together to compile.

- `crates/app/src/editor/element.rs`: `LineLayout`, `DocumentLayout`.
  `request_layout` height becomes `line_height * line_count`. `prepaint`
  iterates `buffer.lines()`, shaping each with runs rebased per D7. `paint`
  walks the lines, painting each at `origin + (0, top_for_line(i))`.
- `crates/app/src/editor/mod.rs`: `last_layout` becomes
  `Option<DocumentLayout>`; `position_for_x` becomes `position_for_point`;
  `scroll_handle` plus the two `div` builder calls from D3.
- `crates/app/src/editor/input.rs`: `bounds_for_range` returns a rect on the
  line holding `range.start`, clamped to that line's end when the range spans
  lines — macOS anchors the IME candidate window to it
  (`platform/mac/window.rs:2193`), and a rect spanning three lines anchors it
  nowhere. `character_index_for_point` resolves y to a line first.
- Selection quads, **four** cases, not three. The draft had no branch for a
  selection that starts and ends on one line, which would have fallen into the
  first-line rule and painted past the selection's end:

  | case | from | to |
  |---|---|---|
  | single line | start x | end x |
  | first of many | start x | line width + newline stub |
  | middle | 0 | line width + newline stub |
  | last of many | 0 | end x |

  The newline stub is what makes a selected line break visible. A selection
  ending at the start of a trailing empty line correctly paints nothing on that
  line — the stub on the line above already shows the newline.

  Zed's single rounded path (`editor/src/element.rs:11160`) is deliberately not
  copied: it needs `PathBuilder` and buys only appearance.
- Tests, on buffers built with embedded newlines: a click at line 2's y lands on
  line 2; a click below the last line clamps to the end rather than panicking;
  `bounds_for_range` on line 2 returns a y inside line 2's band; composition on
  line 2 marks and commits there.
- `check.sh` renders nothing, so this slice also requires running the app and
  looking at it (`CLAUDE.md`) — both for the text itself and to confirm the
  element is not shrunk by the flex parent instead of scrolling.

### Slice 3 — creating and navigating lines

- `crates/editor/src/state.rs`: `LogicalMovement` gains `LineStart`/`LineEnd`,
  keeping `DocStart`/`DocEnd`. `EditorState::insert`.
- `crates/app/src/editor/actions.rs`: `Newline`; `home`/`end` repoint at the new
  variants; `doc_start`/`doc_end` added.
- `crates/app/src/keymap.rs`: `enter` → `Newline`, `cmd-up`/`cmd-down` →
  `DocStart`/`DocEnd`.
- Tests: `editor` `#[test]` per movement on a three-line buffer, including Home
  on an already-at-start cursor and End on an empty line. `app` `#[gpui::test]`
  through `dispatching` for every new binding, including D6's
  exactly-one-newline test.

### Slice 4 — up and down

- `crates/editor/src/selection.rs`: `Goal`; `Selection` carries one;
  `set_head`/`collapse_to` take one.
- `crates/editor/src/state.rs`: every call site states its goal;
  `move_vertically` added beside `move_to`.
- `crates/app/src/editor/{actions,mod,keymap}.rs`: `Up`, `Down`, `SelectUp`,
  `SelectDown`, and the resolution against `DocumentLayout`.
- Tests, through `dispatching` on a ragged buffer (`"aaaa\nbb\ncccc"` — the goal
  column's whole point): down then up returns to the original offset; up on line
  0 goes to offset 0 and clears the goal; down on the last line goes to the end;
  a horizontal move between two vertical moves resets the goal; **an edit**
  between two vertical moves resets it; `shift-down` extends.

## What would make this plan wrong

Written down so review has targets:

- If `line_for_offset` being O(n) shows up in a profile on a 5 000-line note,
  D1's "no cache" is wrong and a `Vec<ByteOffset>` rebuilt in `replace` is the
  answer — but only with that measurement in hand.
- If the layout cache turns out to be keyed on something other than the shaped
  string, D2's caching argument collapses (the API argument survives).
- If `overflow_y_scroll` on a `flex_col` parent shrinks `TextElement` to the
  container instead of scrolling it, D3's "two builder calls" is wrong and the
  element needs `flex_shrink = 0.` or a non-flex wrapper. Visible only by
  running the app.

## What code review changed

Implemented as written, with two defects found in the *verification*, not the
design:

- The click-below-the-last-line test was vacuous. It clicked at y 1150, outside
  the 1536x1095 test window (`gpui/window.rs:81`), so the div's hitbox never
  fired the handler — and it asserted the caret was at the buffer end, which is
  where it already started. Now it moves the caret away first, clicks inside the
  window, and was mutation-checked: deleting either clamp makes it panic with
  `index out of bounds: the len is 2 but the index is 5`.
- Eleven `file:line` citations in `docs/gpui-notes.md` and one in `mod.rs` still
  pointed at the unused registry copy of gpui. See the note below — this is the
  second time that copy has produced wrong citations, so `CLAUDE.md` now forbids
  reading it.

Also fixed: `range_bounds` could return an inverted rect off a stale layout, an
unused `Default` on `Goal`, and two comments whose arithmetic was wrong.

## The gpui source we read was the wrong copy

Found while writing the scroll test. `CLAUDE.md`'s hard rule pointed at
`~/.cargo/registry/src/index.crates.io-*/gpui-0.2.2/`, but `app/Cargo.toml`
depends on a **git rev** (`526c95d`). They are different versions:
`ScrollHandle::max_offset` returns `Size<Pixels>` in the registry copy and
`Point<Pixels>` in the rev, and every line number differs by hundreds. The
macOS platform also moved out of `gpui/src/platform/` into the sibling crates
`gpui_macos` and `gpui_apple`.

Every citation in this change was re-checked against the checkout, and the rule
was rewritten to forbid the registry copy by name.

## What this plan cannot verify

`check.sh` is green for all of it, and two things are still unproven:

- **Enter in the real app.** `TestAppContext::dispatch_keystroke` never enters
  `handle_key_event`, so D6's reasoning is source-read, not test-covered.
- **Stale-layout reads.** gpui force-draws every dirty window as its effect
  queue drains under `cfg(test)`, and `dispatch_keystroke` flushes per
  keystroke. So `last_layout` is refreshed between any two simulated keys, and
  the very window the D5 clamps exist to survive cannot be reproduced in a test.
  The clamps are what make that window harmless; no test will show them working.
