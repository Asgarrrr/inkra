# Soft wrap

Status: implemented (2026-09-23), after red/blue review. "What the review
changed" records where designing it and checking it disagreed; "Corrected in
implementation" records where building it disagreed with both.

A logical line longer than the text column paints on several visual rows. The
column is 70 `ch` wide, centred in the pane, and narrower when the pane is. Up
and down move by visual row. Home, End, `⌘←` and `⌘→` move to the ends of the
visual row; `⌃A` and `⌃E` move to the ends of the logical line.

This replaces "Horizontal scroll and reveal" in `docs/roadmap.md`. That item's
trigger fired on 2026-09-21: one `⌘V` of a URL puts the caret off the right
edge. With wrap there is no right edge to lose it past, so the horizontal half
is closed by removal, not built. A note application wraps prose; none of
Obsidian, Bear or iA Writer scrolls it sideways.

Scope: one wrap mode, one column width, no setting. The width becomes
configurable when a settings system exists; until then it is a constant in
`theme.rs`.

## What gpui gives us

Wrapping inside one logical line is already built, cached and painted:

- `WindowTextSystem::shape_text(text, font_size, runs, wrap_width, line_clamp)`
  returns one `WrappedLine` per `\n`-separated line (`gpui/src/text_system.rs:750`).
  We keep shaping one logical line per call, as today, for the cache reason in
  `element.rs`'s module doc.
- `WrappedLineLayout` (`text_system/line_layout.rs:277`) holds the unwrapped
  `LineLayout` plus `wrap_boundaries`. A boundary names the glyph that
  **starts** the next row (`compute_wrap_boundaries`, `:193`): the candidate is
  the first word character after a space, so a row keeps its trailing space and
  the next row starts on the word.
- A word longer than the width is broken at the glyph that overflows
  (`:259-266`, the arm with no candidate). A URL wraps; it does not overflow.
- `layout_wrapped_line` keys its frame cache on `wrap_width` among the rest
  (`:601-665`).
- `WrappedLine::paint` (`text_system/line.rs:460`) paints every row of the
  line, stepping `line_height` down at each boundary (`paint_line`, `:521`).
- `WrappedLineLayout::position_for_index` and `closest_index_for_position`
  (`line_layout.rs:427`, `:361`) exist, but the caret does **not** use them.
  `position_for_index` resolves an index equal to a boundary to the end of the
  earlier row (`index > line_end_ix` is false at equality, `:443`), the
  opposite of D4. We compute rows ourselves from `wrap_boundaries` and use only
  `unwrapped_layout.x_for_index` and `closest_index_for_x`, which today's code
  already relies on.

What gpui does not give us is the document: rows across logical lines, their
tops, the height of the whole element, and every caret and hit-test rule.

## Decisions

### D1. The column: 70 `ch`, centred, never wider than the pane

`theme::BODY_COLUMN_CHARS: f32 = 70.` The pixel width is
`BODY_COLUMN_CHARS * ch_advance`, where `ch_advance` is the advance of `0` at
the body font and size (`text_system.rs:485`) — the CSS `ch`. It is resolved at
layout time, because it needs the text system.

The body font is gpui's default, `.SystemUIFont`, which is proportional. `0` is
wider than most lowercase letters, so a row of prose holds somewhat more than
70 characters. That is the same approximation every CSS `70ch` measure makes,
and it is accepted: the number is a reading width, not a character count.

The element's width is `relative(1.)` with `max_size.width` set to the column
width, and the editor `div` centres it with `items_center`. So:

- pane wider than the column → the element is the column, centred;
- pane narrower → the element is the pane minus the `div`'s padding.

The wrap width is the element's own width. The padding stays on the `div`,
outside the column.

Clicks in the margins still reach the view: the mouse listeners sit on the
full-size `div`, not on the element. A click left of the column has a negative
element `x` and resolves to the row start; right of it, to the row's
`caret_end` (D4). That is the clamp that already handles a click below the last
line.

### D2. Height is measured, not counted

`TextElement::request_layout` computes `line_height * buffer.line_count()`
today. With wrap, the height depends on the width, which taffy decides. So the
element uses `window.request_measured_layout` (`window.rs:5060`), as gpui's own
`Text` element does (`elements/text.rs:647`).

What taffy 0.13.0 passes the closure, from its source
(`taffy-0.13.0/src/compute/leaf.rs:111-141`): in a sizing pass,
`known_dimensions` as given, **unclamped**; in the final layout pass,
`known_dimensions = NONE` and an `available_space.width` that is already
clamped by `max_size`. The closure may run several times per frame, once per
pass or probe, and taffy may answer the final pass from its cache without
calling the closure again. gpui's wrapper forwards the arguments unchanged
(`gpui/src/taffy.rs:236-264`).

So the closure:

1. resolves the wrap width as `known_dimensions.width`, else a definite
   `available_space.width`, else the column width — then `min`s it with the
   column width, because the sizing pass does not clamp;
2. shapes the document at that width (`shape_document`, D6);
3. stores the layout in a stash shared with `prepaint`,
   `Rc<RefCell<SmallVec<[DocumentLayout; 2]>>>`, **keyed by width** — one entry
   per distinct width, not "the last call";
4. returns `size(width, line_height * row_count)`.

`prepaint` takes the entry whose width is within 1 px of `bounds.size.width`,
the width taffy actually settled on. Taking the last call instead would take
whichever probe ran last, which is not necessarily the one taffy used. If no
entry matches, `prepaint` shapes at `bounds.size.width` and logs it: the
painted rows are then right and the reserved height is one frame stale, which
the next frame's measure corrects. gpui's `Text` panics in the equivalent case
(`elements/text.rs:782-790`); a note editor must not.

The layout engine is cleared every frame (`window.rs:3348`), so the closure
runs every frame. The closure is `'static`: it captures a clone of the
`Entity<EditorView>` and the `TextStyle` read in `request_layout`. It must
capture the style there, because it runs inside `compute_layout`, outside the
`div`'s `with_text_style` scope, where `window.text_style()` returns the
default (`window.rs:2363-2369`).

`flex_shrink = 0.` stays, for the reason its comment gives.

### D3. `DocumentLayout` becomes rows

```rust
struct LineLayout {
    wrapped: WrappedLine,
    range: Range<ByteOffset>,   // excludes the newline, as today
    first_row: usize,           // this line's first row in the document
    rows: SmallVec<[Row; 1]>,   // one per visual row, in order
}

struct Row {
    start: ByteOffset,          // absolute, on a grapheme boundary (D4)
    caret_end: ByteOffset,      // where End and a click past the row land (D4)
    start_x: Pixels,            // x of the glyph gpui broke at (see below)
}

pub(crate) struct DocumentLayout {
    lines: Vec<LineLayout>,     // never empty
    row_count: usize,
    line_height: Pixels,
    style: TextStyle,           // what it was shaped with (D6)
    wrap_width: Option<Pixels>, // None for NameField
}
```

Row starts come from `wrap_boundaries`: boundary →
`runs[run_ix].glyphs[glyph_ix].index`, rebased to the buffer, then snapped
forward to a grapheme boundary (D4). `caret_end` is computed in the same pass.
Both are computed in `shape_document`, which has the `Buffer` the layout was
shaped from, so `DocumentLayout` stays buffer-free afterwards and never
consults a live buffer that has moved on.

The API is row-based. Renamed methods are renamed outright, not aliased:

| Today | After |
|---|---|
| `line_count` | `row_count` |
| `top_for_line(line)` | `top_for_row(row) = line_height * row` |
| `line_for_offset` | kept (the logical line), plus `row_for_offset`: the last row of that line with `start <= offset` |
| `line_for_y` | `row_for_y`, clamped to `row_count - 1` |
| `x_for_offset` | `unwrapped.x_for_index(offset - line.start) - row.start_x` |
| `position_for_offset` | `(x_for_offset, top_for_row(row_for_offset))` |
| `offset_for_position` | `offset_on_row(row_for_y(y), x)` |
| `offset_for_x_on_line` | `offset_on_row(row, x)`: `closest_index_for_x(x + row.start_x)`, clamped to `row.start..=row.caret_end` |
| `range_bounds` | clamped to the **row** holding `range.start` |
| `selection_quads` | one quad per **row**; see D5 |
| `paint` | one `WrappedLine::paint` per line at `top_for_row(first_row)` |

`NameField` never wraps, so its one-row layout answers every row query as its
one-line layout answers line queries today. Its test
`a_one_line_layout_hit_tests_the_field_like_the_editor` (`field.rs:623`) calls
`line_count()` and `top_for_line(0)` (`:637-638`); both calls are renamed in the
commit that renames the methods. The assertions do not change.

### D4. A boundary offset belongs to the next row, and rows start on graphemes

At a wrap, one byte offset `B` is both "after the last character of row N" and
"before the first character of row N+1". Zed stores display points, which tell
the two apart; we store a `ByteOffset` and nothing else. We do not add an
affinity bit to `Selection`: that would reach into `inkra-editor`, and every
`collapse_to` and `set_head` would owe a third argument. Instead, follow Zed's
clipping rule and make the ambiguity unreachable from row N:

- `row_for_offset(B)` is N+1. The caret at a boundary paints at the start of
  the next row. Typing until a word jumps down leaves the caret after that
  word, on the new row, which is where it belongs.
- Nothing that targets row N produces `B`. End, a click past row N's end, and
  an up or down landing on row N all clamp to `row.caret_end`, one grapheme
  before `B`. This is Zed's `WrapSnapshot::clip_point` with `Bias::Left`, which
  steps a row-end point back one column (`editor/src/display_map/wrap_map.rs:969-981`),
  reached from `line_end` (`editor/src/movement.rs:238-252`).

gpui breaks at a **glyph**, and a glyph boundary need not be a grapheme
boundary: a forced mid-word break can fall between a base letter and its
combining mark. A caret there would let Backspace delete the mark alone. So
`Row::start` is snapped forward:
`start = next_grapheme(previous_grapheme(b))`, which is `b` when `b` is already
a boundary and the end of the straddling cluster otherwise. Both are existing
`Buffer` methods; no new one is added. The offsets inside the cluster then
belong to row N, whose `caret_end` is `previous_grapheme(start)`. The mark
still *paints* at the start of row N+1, because gpui paints where it broke —
that is in Deferred.

A row can then be a single grapheme wide — the tail of a long token — with
`start == caret_end`. Every rule above holds for it: the row has one caret
position, and Home and End both land on it.

The visible cost: on a row ending in its trailing space, End lands before the
space, which is invisible. On a mid-word break with no space, End lands before
the row's last visible character. TextEdit places the caret after that
character, on the upper row, using affinity. Zed has our edge. It is accepted
and named here so it is not reported as a bug.

### D5. Selection quads per row

Four cases, as today, over rows `first..=last` of the selection:

- left: `x_for_offset(start)` on the first row, else `0`;
- right: `x_for_offset(end)` on the last row; else the row's right edge, plus
  `NEWLINE_STUB` **only** on the last row of a logical line. A wrap is not a
  newline and does not get the stub.

The row's right edge is the unwrapped `x` of the next row's start minus this
row's `start_x`. It includes the trailing space, so a selection across a wrap
reads as contiguous.

### D6. Only an edit reshapes, and it reveals in the same frame

Today `edit_buffer` and `move_cursor` both call
`reveal_line(buffer.line_for_offset(cursor))` straight after their closure. The
logical line comes from the live buffer; only `line_height` comes from the
stale layout. That is exact, because every row is one line tall.

With wrap, the caret's row depends on how every earlier line wraps and on the
row within its own line. The two gates differ in whether that can have changed:

- **`move_cursor` does not reshape.** It changes no text, so the stored layout
  describes the current text exactly, and `row_for_offset` on it is correct.
  The one way it could describe other text is a buffer swap, and `open_buffer`
  clears it. A resize changes the width, not the text; D2's next frame picks
  that up, and a resize does not reveal today either.
- **`edit_buffer` reshapes.** The keystroke being handled can create a row —
  typing the word that wraps, or pasting a paragraph — and the stored layout
  does not know. After the closure, and after the buffer lease is dropped,
  `edit_buffer` calls
  `shape_document(&buffer, marked, &last.style, last.wrap_width, window)`,
  stores the result as `last_layout`, and reveals the caret's row from it. The
  lease must be dropped first: `shape_document` reads the buffer, and a second
  lease of one entity panics (`gpui/src/app/entity_map.rs:231`). It reads, so
  it takes `buffer.read`, never `as_mut`.

The reveal cannot move to `prepaint`: the `div` has already applied its scroll
offset by then, so it would land a frame late — the reason the scroll-to-cursor
plan put it in the gates. Resolving it inside the measure closure would work
but puts a side effect in a function taffy may call several times with
different widths (D2).

Consequences:

- `edit_buffer` takes `&mut Window`. `move_cursor` and `update_state` do not
  change signature. Every caller of `edit_buffer` already receives a `Window`:
  every action handler (`actions.rs`) and every `EntityInputHandler` method
  (`input.rs`).
- `shape_document` takes the style and the wrap width as arguments instead of
  reading `window.text_style()`: outside a draw the text style stack is empty
  and `text_style()` returns the default, not 18 px (`window.rs:2363-2369`).
  Its three callers pass them: the measure closure (captured style, measured
  width), `FieldElement::prepaint` (`field.rs:393`: `window.text_style()`,
  `None`), and the field test (`field.rs:635`).
- `shape_text` is callable from an action handler: no draw-phase assertion sits
  on its path; the only `debug_assert!` checks for a newline
  (`text_system.rs:638`).
- The gate's shaping lands in the frame cache and survives to the next frame's
  measure: `finish_frame` (`line_layout.rs:585-599`) swaps and clears only at
  the end of `Window::draw` (`window.rs:3348-3349`). The measure pass after an
  edit is a cache hit on every line the edit did not touch.
- `last_layout` describes the current text after an edit, not the previous
  frame's. Every lookup keeps its clamp regardless; the field's doc changes
  from "previous paint" to "last shape".
- `reveal_line(line)` becomes `reveal(offset)` and resolves the row itself.

Cost. `shape_document` allocates a `SharedString` and hashes each line's text
per call, hit or miss (`element.rs:243-248`). A frame pays that once today; an
edit now pays it twice, a caret move still once. For the note sizes in use this
is not measurable. For the 10k–100k-line documents in `crates/text/benches`,
the virtualization trigger already covers it: per-frame shaping cost is what
virtualization removes, and it removes the gate's pass with the frame's. No
benchmark gates this change; `cargo bench` before and after would not see it,
because it times `Buffer`, not shaping.

### D7. Up and down by row

`move_vertically` works on rows: `row_for_offset(cursor)`, target `row ± 1`,
`offset_on_row(target, goal_x)`. The goal is still a pixel `x` in
`Goal::Column`, now relative to the row's start rather than the line's. Past
the first or last row it falls back to `DocStart` / `DocEnd` and drops the
goal, as today. `inkra-editor` does not change.

### D8. Home, End, `⌘←`, `⌘→` by row; `⌃A`, `⌃E` by logical line

- `Home` and `End` move to `row.start` and `row.caret_end`. One press, no
  escalation: pressed again, they stay. Resolved in `app` against the layout,
  arriving in `inkra-editor` as `EditorState::move_to`, which clears the goal.
- `cmd-left` and `cmd-right` bind to the same `Home` and `End` actions. macOS
  text views put visual line start and end there, and the keymap has neither.
  Most Mac keyboards have no Home or End key, so this is the gesture that
  matters.
- `ctrl-a` and `ctrl-e` bind to two new actions, `LineStart` and `LineEnd`,
  resolved by `LogicalMovement::LineStart` / `LineEnd` — the logical line, which
  is what `moveToBeginningOfParagraph:` / `moveToEndOfParagraph:` do in
  NSTextView. Without them the end of a wrapped paragraph has no key.
- `NameField` repeats all four new bindings at its own context, per the keymap
  rule; its handlers resolve all of them with `LogicalMovement`, since a field
  never wraps.
- `LogicalMovement::LineStart` / `LineEnd` are unchanged. Their doc comment,
  which predicts the visual ends moving up to `app`, is updated to say they
  did.

Deviation, stated: in NSTextView, Home and End scroll the document without
moving the caret (`scrollToBeginningOfDocument:`). This editor's Home and End
already moved the caret to the logical line ends before this change; they now
move it to the visual row ends. Making them scroll-only is a separate decision
and is in Deferred.

Zed's second-press escalation to the logical line (`indented_line_beginning`,
`movement.rs:206-231`) was in the first draft and is **not** adopted: it has no
macOS precedent, and `⌃A` / `⌃E` reach the logical line directly.

### D9. What does not change

- `inkra-text` and `inkra-editor`: one doc comment, nothing else. No offset
  kind, no `Selection` field, no movement.
- `scroll::reveal`: it takes a pixel range and does not care whether it came
  from a line or a row.
- The four gates keep their roles. `edit_buffer` gains a `Window` parameter,
  not a responsibility.

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Configurable column width | A settings system exists. | `BODY_COLUMN_CHARS` is the one number to move. |
| Keep the top row stable across a resize | A user loses their place resizing the window. | A resize re-wraps and keeps the pixel offset, so the text at the viewport's top shifts. Zed anchors the scroll to a buffer position. |
| Wrap at the pane width, no column | A user asks for it. | One `min` removed. |
| Shift-Home, Shift-End, `cmd-shift-left/right`, `ctrl-shift-a/e` | A user selects to a line end with the keyboard. | Needs `extend` threaded through D8. None exist for Home and End today. |
| Home and End scroll without moving the caret | A user expects NSTextView's behaviour. | See D8's deviation. |
| Caret affinity at a mid-word wrap | A user reports End stopping one character short on a long URL. | D4's accepted edge. Needs a bias bit on `Selection`, which reaches `inkra-editor`. |
| Paint a wrap on a grapheme boundary | A long unbroken word with a combining sequence is seen split. | gpui breaks at a glyph (`line_layout.rs:259-266`). D4 keeps the caret off the split; the paint still splits. |
| Bidirectional text | A note with a right-to-left script is opened. | The editor has no bidi support before this change. `compute_wrap_boundaries` also measures width as `next_x - last_boundary_x` in glyph order, which assumes x rises with the index; whether CoreText's glyphs for an RTL run do is unverified. |
| Virtualization | As in the roadmap. | Still after wrap. It now iterates rows, and removes the gate's shaping pass with the frame's. |

## Tests

Headless text is measured: `NoopTextSystem::layout_line` places one glyph per
char at `font_size * 0.6` (`gpui/src/platform.rs:1288-1305`), so breaks in a
`#[gpui::test]` are deterministic. The advance depends only on a char's UTF-16
length (`platform.rs:1268-1275`): `0` and `m` agree, a surrogate-pair char is
twice as wide. Fixtures therefore stay in the BMP unless a test is about a wide
char. Each test measures the advance and the column through the text system
rather than hard-coding them. The headless window is 1920 × 1080
(`test_context.rs:274-290`), so the 70 `ch` column (756 px at 18 px) binds in
every test that does not shrink the window.

Written first, failing, in the slice that makes them pass:

1. A line longer than the column occupies two rows: the element is two lines
   tall, and scroll room appears where the rows put it.
2. The column binds in a wide window and the pane in a narrow one; the column
   is centred, its `x` is `(pane - column) / 2`. In both, the painted row count
   times the line height equals the element's height — the check that D2's
   stash handed `prepaint` the layout taffy sized.
3. The caret at a boundary offset paints at `x = 0` on the next row.
4. Round trip: for every grapheme boundary `o` of a fixture with two wrapped
   lines, a mid-word break and a one-grapheme last row,
   `offset_for_position(position_for_offset(o)) == o`.
5. A click past the end of row N lands on row N's `caret_end`, not on `B`.
6. A selection across one wrap and one newline paints three quads, and only
   the newline's row carries the stub.
7. `bounds_for_range` for a range on the second row of a line returns that
   row's `y`.
8. Down from row 0 of a wrapped line lands on row 1 of the **same** line at the
   goal column; the next Down leaves the line. Up mirrors it. A run of Downs
   across a short row keeps the goal, as the existing goal test does across
   lines.
9. `home`, `end`, `cmd-left`, `cmd-right` land on the visual row's ends and are
   idempotent; `ctrl-a`, `ctrl-e` land on the logical line's ends. One
   `simulate_keystrokes` test per binding per context, `EditorView` and
   `NameField`.
10. Typing the word that wraps the caret onto a new row, with that row just
    inside the reveal margin, leaves exactly `MARGIN_LINES` rows below it after
    the keystroke. With the gate's reshape removed, the reveal is computed from
    the stale layout and leaves one row fewer — the test fails. (A
    `#[gpui::test]` cannot observe "before the draw": `simulate_keystrokes`
    draws before returning. It asserts the margin, which is what differs.)
11. A paste of a long URL at the bottom of the viewport reveals the caret on the
    URL's last row.
12. A fixture with a cluster split by a forced break: no caret position, reached
    by arrows, Home, End or a click, lies inside the cluster.

`editor/tests/scroll.rs` stays green, with one test's premise gone:
`revealing_never_scrolls_horizontally` builds 2000-char lines to overflow
sideways, and they now wrap into 29 rows. Its assertions still hold. The test is
rewritten to assert that a wrapped document has no horizontal scroll room at
all, which is the claim it now supports, and the commit says so.

Look (`cargo run -p inkra-app --features shot --example shot`): two scenes,
`editor-wrapped` (a paragraph and a URL, caret on a wrapped row) and
`editor-wrapped-selection` (a selection across a wrap and a newline). The
existing `Stage` API expresses both (`shot/scenes.rs:139-146` is the pattern).
Plus a real window with a resize, which no test reaches.

`cargo mutants` covers `inkra-text` and `inkra-editor`, which change by one doc
comment; it is not rerun.

## Slices

Each ends with `./scripts/check.sh` green and one commit.

1. **`edit_buffer` takes a `Window`; `shape_document` takes its style and
   width.** No behaviour change, `wrap_width: None` everywhere. The existing
   suite is the test — the one slice with no new failing test, because it adds
   no behaviour. `./scripts/test-names.sh` before and after, and the diff
   empty.
2. **Rows, drawn.** Rows in `DocumentLayout`, measured height, the column, the
   stash, paint, caret position, reveal by row, up and down by row. Tests 1, 2,
   3, 8, 10, 11; the `scroll.rs` rewrite; scene `editor-wrapped`. Up and down
   come here, not later, so no commit on the branch moves the caret by logical
   line past a wrapped row.
3. **Rows, hit.** Click, selection quads, IME bounds, the grapheme snap. Tests
   4, 5, 6, 7, 12; scene `editor-wrapped-selection`.
4. **Line ends.** Home, End by row; the four new bindings in both contexts.
   Test 9.

Docs ride with the slice that makes them true: CLAUDE.md's gate description
and don't-recreate rows (`Row`, `row_for_offset`, `offset_on_row`,
`BODY_COLUMN_CHARS`, the boundary convention) in slices 2 and 3; the roadmap
(horizontal scroll closed by removal, soft wrap shipped) and `gpui-notes.md`
in slice 2.

## What the review changed

- **D6 reshaped on every caret move.** The first draft had `move_cursor` and
  `update_state` take a `Window` and reshape too. A caret move changes no
  text, so the stored layout is already exact; only `edit_buffer` reshapes.
- **D8 had a second-press escalation**, Zed's. It has no macOS precedent, and
  on a one-grapheme row it collapsed to one press, breaking its own contract.
  Replaced by `⌃A` / `⌃E`.
- **D4 claimed no caret could land inside a cluster**, but only clipped the row
  end. The row start is now snapped too.
- **D2 kept "the last measured layout"**, as gpui's `Text` does. taffy may call
  the closure several times per frame and answer the final pass from its cache,
  so the last call is not necessarily the one used. The stash is now keyed by
  width, and a miss reshapes instead of panicking.
- **D2 left taffy's clamping as an open question.** Read from
  `taffy-0.13.0/src/compute/leaf.rs:111-141`: the sizing pass does not clamp,
  the layout pass does. The closure clamps itself either way.
- **The other two open questions** — `shape_text` outside a draw, and the frame
  cache surviving a gate — were answered from the source (D6) and are closed.
- **D3 said the field test would stay unchanged**, while renaming the two
  methods it calls. The calls are renamed; the assertions are not.
- **Test 10 claimed to observe the offset "before any draw"**, which a
  `#[gpui::test]` cannot. It now asserts the margin, which the stale reveal gets
  wrong.
- **Slice 2 was the whole element in one commit**, and left up and down on
  logical lines. Split into drawn and hit; up and down moved into the first.
- Four citation line numbers were wrong and are corrected.

## Corrected in implementation

- **D4 snapped row starts with `Buffer::previous_grapheme` / `next_grapheme`.**
  Both scan the whole document per call (`text/src/buffer.rs:341-355`), and
  `rows` calls them once per wrap per frame: the 200 × 2000-char fixture in
  `scroll.rs` never finished. The snap now runs `GraphemeCursor` over the
  line's own text, which is why `unicode-segmentation` moved to
  `[workspace.dependencies]` and `inkra-app` depends on it.
- **D2 returned the wrap width for every probe.** For a min-content probe that
  width becomes the flex item's automatic minimum, and beside the sidebar the
  editor overflowed any pane narrower than the column by up to 20 px. Every
  test passed, because every editor test roots the window at `EditorView`,
  which is never asked; the shot showed it. The closure now reports zero
  min-content width, and `shell/tests/column.rs` roots at `AppShell` to pin it.
- **D2 shaped the document once per probe.** taffy asks six times per frame
  here, at two distinct widths. The closure now answers a width it already
  shaped from the stash.
- **D3 took `start_x` as the unwrapped x of `start`.** `start` is snapped past
  a split cluster; gpui starts painting the row at the glyph it broke at. So
  `start_x` is that glyph's `position.x`, and caret x and painted glyphs share
  one origin.
- **Slices 2 and 3 were one commit.** The hit-testing methods are the row
  methods; converting the layout converted them. Their tests landed with it.
