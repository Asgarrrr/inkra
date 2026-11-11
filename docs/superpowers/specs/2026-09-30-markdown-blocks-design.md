# Markdown blocks: quote bars, code backgrounds, task boxes

Status: **S1 merged** (#56). S2 is superseded by
`2026-11-07-code-blocks-design.md`. S3 refreshed 2026-11-11, see "S3 as
planned"; not built.

## Why

Live preview left three constructs as dimmed syntax: `> ` on a quote, the
fence lines of a code block, and the `[ ]` of a task (live-preview spec, D2).
Its Deferred table holds them under one row, "Task boxes as checkboxes, quotes
as a bar, code blocks with a background and hidden fences", with the trigger
"The user asks". The user asked on 2026-09-30.

The three share one missing capability: painting beyond text. A quote needs a
bar, a code block a background, a task a box. Two of them also need a
**gutter**, a hanging indent that keeps wrapped rows clear of the bar or box.

## Decisions taken

Taken by the user on 2026-09-30.

1. **A task box is clickable.** A click toggles it; one click is one undo step.
2. **A hidden fence line stays a row.** It shows no text, and the code
   background covers it, so it acts as the block's top and bottom padding. No
   row has zero height, and no reveal moves the text vertically.
3. **A quote's `>` and a task's marker move into a gutter.** The text after
   them, wrapped rows included, starts at the gutter's right edge. A revealed
   `>` or `- [ ]` paints inside the gutter, so no reveal moves the text
   horizontally either.
4. **`inkra-markdown` names the structures; `app` paints them.** The crate
   stays free of pixels.

An advisor review (Opus) of the first draft changed four things: the gutter
(the first draft put a bar at the `>`'s `x`, which wrapped rows cross), the
click moving the caret (else the edit scrolls to a caret off screen), three
typed vectors in place of one `Block` enum, and explicit fence conceals (the
gap rule swallows a `>` before a closing fence).

## D1. The data: three typed vectors

```rust
pub struct Markdown {
    pub spans: Vec<Span>,
    pub conceals: Vec<Conceal>,
    pub headings: Vec<Heading>,
    /// Sorted by `range.start`; one per line that has a quote or task prefix, lazy lines included.
    pub gutters: Vec<Gutter>,
    /// Sorted by `range.start`; the whole block, fences included, clipped before its last `\n`.
    pub code_blocks: Vec<CodeBlock>,
}

pub struct Gutter {
    /// From the line's start to where its text starts; empty on a lazy line.
    pub range: Range<ByteOffset>,
    /// Each `>` on the line, outermost first; `depth` counts them on a lazy line too.
    pub quotes: Vec<ByteOffset>,
    pub depth: usize,
    pub task: Option<Task>,
}

pub struct Task {
    /// The three bytes `[ ]`, `[x]` or `[X]`.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}

pub struct CodeBlock {
    pub range: Range<ByteOffset>,
}
```

One vector per structure follows `headings`. No consumer wants quotes, code
blocks and tasks together, and a task box is a three-byte slot, not a block.

`depth` is a `usize`: `parse` never panics, and 256 `>` must not overflow.

**What a gutter covers.** A line gets a `Gutter` when it is inside a quote, or
when it starts a task item. Its range runs from the line's start through:

- each `>` of the quotes that contain the line, and the one space after each;
- for a task item's first line, the list indent, the bullet or number, its
  space, the checkbox, and the space after it.

A plain list item keeps its bullet inline, as today. `> - a` has a gutter over
`> ` only. The bullet's `•` conceal is unchanged.

**A lazy line** (`b` in `> a\nb`) is inside the quote but has no `>`. Its
gutter has an empty range, empty `quotes`, and the quote's `depth`. `app` lays
it out with the geometry of the gutter above it (D3).

**Deriving the `>` positions from the parse, not from the bytes.** A scan of
`[ >]*` would hide the `>` of the code line in `> ```\n> > x\n> ````. `parse.rs`
knows which `BlockQuote` elements are open on each line and takes one `>` for
each. The same computation gives the container prefix that the fence conceals
skip (D2).

## D2. Conceals

| Construct | Element (what reveals it) | Marker | Replacement |
|---|---|---|---|
| `>` | the `>` character | the `>` | `""`, painted in the gutter while revealed |
| task marker | the checkbox | the list marker through the checkbox | `""`, the box painted in the gutter |
| opening fence | the whole code block | the fence and its info string, after the container prefix | `""` |
| closing fence | the whole code block | the fence, after the container prefix | `""` |

**Task item markers are decided once.** Today `bullet()` emits the `•`
conceal at `Start(Item)`, and `TaskListMarker` arrives later, after
`Start(Paragraph)` in a loose list. Patching a conceal already emitted would
be a special case. Instead, the item's marker conceal is emitted at the item's
first inner event: a `TaskListMarker` makes it the task marker; anything else
makes it the `•` bullet. One conceal per item, chosen by what the item is.

**The checkbox is the last three bytes of `TaskListMarker`'s range.** That
range can start with up to three spaces (pulldown-cmark 0.13.4,
`scanners.rs:380`).

**Fence conceals are explicit**, emitted as `heading()` emits its marker. The
gap rule (D3 of the live-preview spec) cannot do it: the closing fence's gap
starts at the line start, so `"> ```"` is one marker, and hiding it would
overlap the `>` conceal. `DisplayLine::new` asserts that markers never
overlap.

A fence without a closing fence runs to the end of the note: only the opening
fence is concealed. An indented code block has no fence to conceal.

The live-preview invariants hold unchanged: every marker lies in one line,
markers never overlap, and no active marker touches the selection (D4).

## D3. The gutter: a hanging indent

`LineLayout.hanging: Option<Hanging>` becomes `prefix: Option<Prefix>`:

```rust
enum Prefix {
    /// A heading's opening marker, left of the column (live-preview spec, D12).
    Margin(Hanging),
    /// A quote or task prefix; the line's text starts at `width`.
    Gutter(GutterLayout),
}
```

A line has at most one: a heading inside a quote keeps its inline marker
conceal, as D12 already accepts.

**The gutter never enters the shaped text.** As with D12, its range goes to
`DisplayLine` as hidden in both states. The line's text is shaped with a wrap
width of `column − width` and painted at `x + width`. Every row of the line,
not only the first, starts at the gutter's edge.

**Its width does not depend on the reveal state or on the line's font.** It
is the width of the gutter's source text shaped at the body size, measured
once per line. A heading inside a quote is shaped larger, but its gutter
matches the body lines around it, so bars of equal depth align. A lazy line
takes the width and bar positions of the gutter above it.

**What paints in the gutter:**

- a bar per quote level, at the `x` of that level's `>`, over all the line's
  rows. Bars of consecutive lines touch, so a quote reads as one bar;
- a revealed `>`, where it was shaped;
- a task box, centred where the checkbox was shaped, with a check when
  checked; the revealed `- [ ]` text instead of the box when revealed.

This is Zed's `next_indent`: a wrapped row restarts at an indent
(`vendor/zed-reference/crates/editor/src/display_map/wrap_map.rs:655-658`).

**Every pixel↔offset conversion adds the width.** `x_for_offset`,
`offset_for_x`, `hit_test` and the selection and caret quads read the line's
prefix, as they read `Hanging` today. A click in the gutter lands on the
text's start, as a click in the margin does.

**Performance.** One extra `shape_line` per gutter line per shape, at the body
size. `shape_document` already shapes every line once; measure a 50 KB note of
quotes before and after with the off-screen harness.

## D4. The code background

A code block paints one rounded quad from its first line's top to its last
line's bottom, all rows included. It spans the text's width, from the gutter's
edge to the column's right, with a small outset. A hidden fence line is an
empty row inside it (decision 2).

`DocumentLayout` gains `backgrounds: Vec<Bounds<Pixels>>`, built in
`shape_document` as the lines are laid out, so no frame recomputes it. Blocks
and lines are both in document order: one cursor walks both. Finding each
block's rows by `line_for_offset` would scan the lines once per block.

A fence line that wraps when revealed (an info string longer than the column)
adds a row on reveal. Accepted.

## D5. Paint order

`text_input::paint` paints, in order:

1. code backgrounds;
2. the selection;
3. the text, the gutters and the margins;
4. the caret.

Today the selection paints before the text (`text_input/paint.rs:66-69`). A
background painted with the text would cover the selection inside a code
block, so backgrounds come first.

## D6. Toggling a task

**Hit test.** `DocumentLayout::task_at(point) -> Option<Range<ByteOffset>>`
returns the checkbox under the point, only while its box is painted. A
revealed checkbox is text, and a click on it places the caret.

**The click moves the caret.** A click on a box toggles it and places the
caret at the start of the item's text. Every edit goes through `edit_buffer`,
whose `after_edit` reveals the cursor (`editor/mod.rs:288-291`). With the caret
left where it was, perhaps off screen, a click would scroll the view away.
Obsidian has this defect (forum thread 62830). The item's text start does not
touch the checkbox, so the box stays painted.

**The edit is one step.** `inkra-editor` gains a one-step replace beside
`paste` and `cut` (`crates/editor/src/state.rs:137-169`): finalize, replace
the middle byte, finalize. `app` passes the new byte: `x` for an unchecked
box, a space for a checked one. `mouse.rs` only hit-tests and dispatches.

After `⌘Z`, the caret lands on the edit, inside the checkbox, so the box shows
as `[ ]` text until the caret leaves. Accepted: the reveal rule says so.

## D7. Theme

`theme.rs` gains the colours `code_background`, `quote_bar`, `task_box` and
`task_check`, and the sizes `QUOTE_BAR_WIDTH` and `TASK_BOX_SIZE`. No literal
colour or size outside it.

## Deviations from Zed

- **The checkbox's place is a shaped width, not a reserved one.** Zed reserves
  a measured element's width in the wrap map (`wrap_map.rs:638-642`).
  `shape_text` has no inline element, so the box paints over the width its
  source text shaped at. It sits in the gutter, where no text follows it on
  the same row, so the width only sets the gutter's edge.
- **`next_indent` is a partial precedent.** Zed indents the wrapped rows of a
  line by its own leading whitespace, inside the wrapped text, and leaves the
  first row at 0 (`wrap_map.rs:655-658`). The gutter moves every row, the
  first included, by a width that is not in the shaped text.

## Tests

`inkra-markdown`, by example:

- nested quotes `> > x`, and a lazy line `> a\nb`;
- a fence inside a quote, closed and unclosed, and `> ```\n> > x\n> ````;
- an indented code block;
- `- [ ] a`, `- [x] a`, `- [X] a`, `1. [ ] a`, a task in a quote, a task in a
  loose list, a task marker with leading spaces;
- a plain bullet next to a task keeps `•`.

`inkra-markdown`, proptest: the existing conceal properties cover the new
markers; `gutters` and `code_blocks` are sorted, inside the text, and never
end on a `\n`; a gutter's range starts its line.

`app`, `#[gpui::test]` against `MemFs`:

- a click on a box toggles `[ ]` to `[x]` and back, and moves the caret to the
  item's text start;
- `⌘Z` after a toggle restores the text;
- a click on a revealed checkbox places the caret and edits nothing;
- a toggle with the view scrolled away from the old caret does not scroll;
- hiding and revealing a fence keeps `row_count`;
- a wrapped quote line's second row starts at the gutter's edge;
- revealing a `>` or a task marker moves no text.

Visual: a `shot` scene, `markdown_blocks`, rendered and inspected after each
slice, with nested quotes, a wrapped quote line, a heading in a quote, a code
block in a quote, and checked and unchecked tasks.

## Slices

Each slice is vertical: data, layout, paint, tests, scene.

1. **S1, the gutter and quotes** (~2 d). `Gutter` with `quotes` and `depth`,
   the `>` conceal, `Prefix`, the hanging indent, bars.
2. **S2, code blocks** (~1 d). `CodeBlock`, fence conceals, backgrounds, the
   paint order.
   - Binding from S1's review: `backgrounds` is built in `shape_document`
     beside `bars`, and a separate `paint_backgrounds` paints it before the
     selection.
3. **S3, tasks** (~1.5 d). `Task`, the item's marker decided once, the box,
   `task_at`, the one-step replace, the click.
   Binding from S1's review:
   - **Each part of a prefix reveals on its own.** In `> - [ ] a` the `>` part
     and the task part reveal separately. Split the prefix's conceals a second
     time, at the task marker's start, and paint the gutter in pieces
     (`ShapedLine::split_at`,
     `vendor/zed-reference/crates/gpui/src/text_system/line.rs`).
   - **Box bounds are built in shape**, beside `bars`; `task_at` scans that
     vector.
   - **A lazy line under a quoted task carries only the quote part's width.**

## As built (S1)

S1 shipped on `feat/markdown-blocks-s1`. Each item below deviates from the text
above or fills a gap in it.

**Spec corrections from the plan**
(`docs/superpowers/plans/2026-09-30-markdown-blocks-s1.md`):

1. **Only a leading quote gets a gutter.** A leading quote is one whose
   ancestors are all quotes. In `- > a` a gutter from the line's start would
   swallow the bullet. A quote inside a list item keeps the inline dimmed
   `> `. `depth` counts leading quotes only; `> - > a` gets a gutter over the
   first `> ` only.
2. **The `>` conceals of one line share one element**, from the gutter's start
   through the line's last `>`. A gutter then has one state: it paints its one
   shaped text or nothing. A caret after the last `>`'s space reveals nothing.
3. **A click in a revealed gutter lands on the offset under it.** While
   concealed, a click left of the text lands on the text's start. While
   revealed, the gutter maps `x` through its shape, as D12's margin does.
4. **A lazy line is any gutter with fewer `>` than its `depth`.** In
   `> > a\n> b`, `b` holds one `>` of two. A gutter holding all its `>`
   measures its geometry; any other carries the last measured one.
5. **Where a `>` may stand** follows CommonMark 5.1: up to three spaces before
   each `>`, a fourth makes the rest text, and one space or tab after it joins
   the gutter.
6. **The scenes are `markdown-blocks` and `markdown-blocks-revealed`**,
   kebab-case as every catalogue name.
7. **`Gutter` has no `task` field in S1.** S3 adds it with its reader.
8. **Files split before ~400 lines:** `layout/prefix.rs` and `layout/paint.rs`,
   `crates/markdown/src/gutters.rs`, `parse/tests/gutters.rs`,
   `shot/scenes/markdown_blocks.rs`.
9. **`next_indent` is a partial precedent** (Deviations from Zed).
10. **D5's paint order is S2's.** In S1 the bars paint first, before the text,
    and after the selection. A bar covers a 2 px strip of a selection in the
    gutter.

**Decisions taken during the build**

- **A gutter's bytes are styled by the gutter alone.** pulldown-cmark's `Code`
  and `InlineHtml` ranges run across lines and cover the next line's container
  `>`. The gap rule gives a container `>` its inline element's style
  (`QUOTE|STRONG|MARKER` in `> **a\n> b**`). So the gutter could be shaped in
  the code font or in bold. `gutters::cut` runs once the gutters are known: it
  cuts every span around each gutter and gives the gutter one
  `QUOTE | MARKER` span. It names no event kind. A `debug_assert!` holds that
  no gutter crosses a `\n`.
- **`GutterCarry` holds what gutters share within one `shape_document`**: the
  geometry the last full gutter measured, carried to lazy lines, and each
  distinct gutter text's geometry (width, bar `x`s). Geometry depends only on
  the text at the body size, so each distinct text is shaped once per call;
  most gutters are `> ` or `> > `. Nothing is kept across calls.
- **A concealed gutter keeps no shape**, as a concealed margin keeps none.
  Only a revealed gutter keeps its shape, to paint it. An offset inside a
  concealed gutter maps to the text's start. No caret or selection reaches
  one: touching is inclusive, so an endpoint inside a gutter reveals it.
- **One reveal rule for both prefixes.** Conceals are sorted by
  `marker.start`. `line_prefix` splits the line's conceals once at the
  prefix's end. The prefix is revealed when a conceal in its part is inactive;
  the text gets the other part, unfiltered. This holds because a gap conceal
  skips a line's leading `[ \t>]*` and a heading's own marker is the only
  conceal before its end. A `debug_assert!` checks it.
- **Bars are built in shape**, as D4 builds backgrounds:
  `DocumentLayout.bars`, one per level per line, from the line's top to the
  next line's. `paint` only iterates them.
- **One cursor rule for every per-line cursor:** spans, conceals and gutters
  take the items with `start <= line end`. The next line's items start one
  byte later at the earliest, so the rule also holds for an empty item.
- **`Unrepeated` moved to `parse/unrepeated.rs`**, so `parse.rs` stays under
  400 lines. pulldown-cmark 0.13.5 later made it unnecessary, and it is gone.
- **gpui's `closest_index_for_x` returns the length past the last glyph's
  start** (`text_system/line_layout.rs:78-104`). In a revealed `"> "`, a click
  between the space's start and the text lands on the text's start. D12's
  margin behaves the same.

**Measurements** (same session, base `4829e77`, interleaved rounds of three
runs). The note is dense: five of its six lines have a gutter, four with a `>`.

| | base | S1 | change |
|---|---|---|---|
| `perf-quotes` keystroke median, first build (every gutter shaped) | 1.94 / 1.99 / 2.78 ms | 2.40 / 2.65 / 3.41 ms | +23 % to +33 % |
| `perf-quotes` keystroke median, as shipped (18 runs each) | 1.90 ms | 1.98 ms | +4 % (paired rounds: −4 % on average) |
| `markdown_parse/5000` | 27.1 µs | 29.7 µs | +10 % |
| `markdown_parse/50000` | 249 µs | 272 µs | +9 % |
| `markdown_parse/500000` | 3.00 ms | 3.23 ms | +7 % |

The first build exceeded the plan's 20 % bar. A profile (`sample`, 3000
keystrokes) put `line_prefix` at about 20 % of `shape_document`: two thirds
the gutter's `show` (`line_runs`, a `String`, a cached `shape_line` lookup),
a quarter `memmove` of the `ShapedLine`. A `ShapedLine` holds its
`SmallVec<[DecorationRun; 32]>` inline, about 3 KB. Keeping no shape for a
concealed gutter and measuring each distinct text once brought the keystroke
within noise of base. After, `line_prefix` is about 14 % of
`shape_document`: no shaping on the concealed path, about 6 % map lookups and
`Rc` clones, about 6 % `memmove` of the line's `Option<Prefix>`. Boxing the
shape would remove the last part; the measurement does not call for it.

`cargo mutants -p inkra-markdown -f gutters.rs -f parse.rs`: 269 caught,
11 timeouts (loops that never end), 70 unviable, 1 missed. The missed one is
equivalent: `<` to `<=` in `cut`'s `next_if` takes a gutter that starts where
the span ends one span earlier, which pushes the same span then the same
gutter.

## S3 as planned (2026-11-11)

D1 to D7 were written on 2026-09-30. The code moved since: semantic theme
tokens, the code blocks spec, the centred column, a scroll per code block, and
line ceilings. Each item below corrects the text above against `main` on
2026-11-11, or fills a gap in it. A spike on branch `spike/task-boxes` printed
pulldown-cmark's events and today's conceals for 30 task cases. It never
merges.

### Parser facts

`Cargo.lock` holds pulldown-cmark 0.13.6 (`Cargo.lock:4042-4043`). Results of
the spike:

| Text | `TaskListMarker` range | Before it, after `Start(Item)` | Task? |
|---|---|---|---|
| `- [ ] a`, `- [x] a`, `- [X] a` | 2..5 | nothing | yes; `[x]` and `[X]` are checked |
| `* [ ] a`, `-\t[ ] a` | 2..5 | nothing | yes |
| `-  [ ] a` | 3..6 | nothing | yes |
| `-    [ ] a` | 5..8 | nothing | yes |
| `  - [ ] a` | 4..7 | nothing | yes |
| `1. [ ] a` | 3..6 | nothing | yes |
| `> - [ ] a` | 4..7 | nothing | yes |
| `> > - [ ] a` | 6..9 | nothing | yes |
| `- [ ] a\n\n- [x] b` (loose) | 2..5 | `Start(Paragraph)` 6..8 | yes |
| `- [ ] a\n\n  b` (loose) | 2..5 | `Start(Paragraph)` | yes |
| `- [ ]\n`, `- [x]\n` | 2..5 | nothing; `End(Item)` follows | yes, with no text |
| `- [ ]`, `- [x]` at the end of the text | none | `Text` | no |
| `- [ ]a` | none | `Text` | no |
| `- > [ ] a` | none | `Start(BlockQuote)` | no |
| `- [ ] [ ] a` | 2..5 | nothing | the first only |
| `- - [ ] a` | 4..7, on the inner item | the outer item sees `Start(List)` | the inner item only |
| `- a\n  - [ ] b` | 8..11, on the inner item | nothing | the inner item only |
| `-\n`, `- ` | none | `End(Item)` comes first | no |

In every case the range is exactly the three checkbox bytes. The source does
not rule out a leading space: the event starts at `after_marker_index`
(`pulldown-cmark-0.13.6/src/firstpass.rs:148`), and the scanner skips up to
three spaces before `[` (`scanners.rs:378-380`). Keep the rule of D2, "the
last three bytes of the range": it is right in both cases.

Today `- [ ] a` gets one conceal, the `•` at 0..1. The gap rule styles
`- [ ] ` as one `MARKER` span and conceals nothing else. In `> - [ ] a` the
span `> - [ ] ` is `QUOTE | MARKER`, and the gutter covers `> ` only.

### Corrections

1. **The edit uses `Buffer::transact`, not "finalize, replace, finalize"**
   (D6). The repo rule says a one-step gesture uses `Buffer::transact`
   (`CLAUDE.md`, "Code"). `edit_ungrouped` does it
   (`crates/editor/src/state.rs:77-82`). See "The edit" below.
2. **`state.rs:137-169` moved.** `edit_ungrouped` is at
   `crates/editor/src/state.rs:78`, `cut` at 147, `paste` at 153.
3. **`after_edit` moved** from `editor/mod.rs:288-291` to
   `crates/app/src/editor/mod.rs:424-427`. It still reshapes and reveals the
   cursor (`Reveal::Minimal`), so D6's reason for moving the caret holds.
4. **The paint order lives elsewhere.** `text_input/paint.rs:66-69` is stale:
   `fn paint` starts at `crates/app/src/text_input/paint.rs:77`, and
   `layout.paint_backgrounds` runs at line 88, before the selection (89-91).
   S3 changes no paint order: the box paints with the gutter, after the
   selection.
5. **Colours are semantic tokens** (D7). The list is the `tokens!` macro in
   `crates/app/src/theme/colors.rs` (`md_quote_bar` at line 62), with one
   value per built-in in `crates/app/src/theme/builtin.rs` (lines 42 and 85).
   Sizes stay in `crates/app/src/theme.rs` (`QUOTE_BAR_WIDTH` at line 289).
   See "Theme" below.
6. **pulldown-cmark is 0.13.6**, not 0.13.4. The range was exactly three bytes
   in every case of the spike (Parser facts). `scanners.rs:380` is now inside
   `scan_task_list_marker` (`scanners.rs:378`).
7. **D2's "first inner event" picks the bullet in a loose list.** There
   `Start(Paragraph)` comes before `TaskListMarker` (Parser facts). The rule
   becomes: the item's marker is decided at the first event after
   `Start(Item)` that is not one `Start(Paragraph)`. Zed uses the same rule
   (`vendor/zed-reference/crates/markdown/src/markdown.rs:3820-3836`). `parse`
   holds one pending item start; the next event that is not a
   `Start(Paragraph)` emits the task conceal or the `•`, then clears it. The
   paragraph's own handling is unchanged. Conceals are sorted by
   `marker.start` after the loop (`crates/markdown/src/parse.rs:122`), so a
   late conceal lands in order. Rejected: lookahead over the event stream. The
   loop reads `into_offset_iter` once (`parse.rs:63`), and one pending slot
   decides the same thing.
8. **No pointer-cursor precedent exists.** The plan named the link hover as
   the pattern. `on_mouse_move` only extends a drag selection
   (`crates/app/src/editor/mouse.rs:64-79`), and `crates/app/src` sets no
   `cursor_style` and no `PointingHand`. See open question 4.
9. **"Box bounds are built in shape, beside `bars`"** meets the line
   ceilings. `crates/app/src/editor/layout/shape.rs` (464 lines) and
   `layout/mod.rs` (412) are listed in `scripts/line-ceilings.tsv`; a listed
   file must stay exactly at its ceiling (`scripts/line-ceilings.sh:36-41`).
   See "The module" below.
10. **S2 is superseded** by `2026-11-07-code-blocks-design.md`, built
    2026-11-07. A box's bounds depend on none of the geometry that came later:
    - *The centred column.* `DocumentLayout::margin()`
      (`layout/mod.rs:117-119`) feeds only a heading's `Prefix::Margin`
      (`shape.rs:142`). Bars are built in layout coordinates and painted at
      `origin + bar` with no margin term (`layout/paint.rs:65-67`). Box bounds
      do the same.
    - *A code block's scroll* (`layout/scrolls.rs`). `x_on_row` subtracts the
      block's scroll only off the prefix (`layout/mod.rs:278-286`), and paint
      draws the prefix unscrolled (`layout/paint.rs:74-86,109-120`). A task's
      line is never in a code block anyway: after a `TaskListMarker`,
      pulldown-cmark parses the rest of the line as a paragraph
      (`firstpass.rs:159-161`).
    - *A task inside a quote inside a list.* `- > [ ] a` is no task (Parser
      facts), and a quote inside a list item gets no gutter (As built,
      item 1). See item 13.
11. **The task part of a gutter keeps its gap style.** `gutters::cut` gives a
    whole gutter one `QUOTE | MARKER` span
    (`crates/markdown/src/gutters.rs:95`; As built, "A gutter's bytes"). A
    task's gutter outside a quote must not read as a quote. `cut` covers the
    quote part only, through the last `>` and its space. The task part keeps
    the span the gap rule gives it today: `MARKER`, or `QUOTE | MARKER` inside
    a quote. No inline element crosses an item's start, so the task part has
    nothing to cut.
12. **Two checks that hold "own conceals are the quotes" change.** The
    `debug_assert!` in `line_prefix` (`shape.rs:398-405`) becomes
    `own.len() == gutter.quotes.len() + usize::from(gutter.task.is_some())`.
    The G-property in `crates/markdown/src/parse/tests/gutters.rs:238-247`
    (every conceal in a gutter is one byte, and they are the quotes) admits
    one more conceal: the task marker, last, from the bullet through the
    checkbox.
13. **A task gets a gutter only when its marker leads its line.** Before the
    bullet or number, the line may hold only leading quotes and spaces.
    `  - [ ] a` and `> - [ ] a` qualify. In `- - [ ] a` the inner task follows
    the outer item's `- `; it gets the `•` conceal, and its checkbox stays
    dimmed text. Rejected: a gutter from the line's start, which would swallow
    the outer bullet, as As built item 1 rejected for `- > a`.

### The data and the conceal

`Gutter` gains `task: Option<Task>` with D1's `Task`. This is the reader that
As built item 7 waited for. A line gets a `Gutter` when it is quoted or
starts a task item (item 13). A task gutter's range ends after the checkbox
and the one space or tab after it; with no text (`- [ ]\n`), it ends at the
checkbox.

The task conceal follows D2's row. Its element is the checkbox. Its marker
runs from the bullet or number through the checkbox. Its replacement is `""`.
An ordered task hides its number with the rest of the marker. A caret at the
item's text start does not touch the checkbox, so the box stays painted.

### The prefix in pieces

Today a gutter has one state. `line_prefix` splits the line's conceals once,
at the prefix's end (`shape.rs:381-427`). Any inactive own conceal reveals
the whole prefix. `GutterCarry::layout` keeps a `ShapedLine` only for a
revealed gutter (`layout/prefix.rs:129-174`). `Prefix::shown` returns it at
`x = 0` (`prefix.rs:63-71`), and paint draws it at the line's origin
(`paint.rs:109-120`).

S3 splits the own conceals a second time, at the task conceal. Conceals sort
by `marker.start` and every `>` precedes the bullet, so the task conceal is
the last own conceal. The quote part and the task part each get a revealed
flag. A revealed part shapes the whole gutter text once, through the existing
`show`, and splits it with `ShapedLine::split_at`
(`vendor/zed-reference/crates/gpui/src/text_system/line.rs:196-259`). The
piece before the split keeps its positions. The piece after starts at `x = 0`
and paints at the split's `x`. A part that is concealed keeps no shape, as
today.

`Geometry` (`prefix.rs:120-125`) gains the checkbox's `x` span, measured with
the bars from the same shape (`Geometry::measure`, `prefix.rs:176-188`). It
also keeps the quote part's width apart. A lazy line under a quoted task
carries the quote part's width only, per S1's review. `GutterCarry` measures
each distinct gutter text once, so `- [ ] ` and `- [x] ` cost one measure
each per `shape_document`.

### The module

A new `crates/app/src/editor/layout/marks.rs` holds what paints in a gutter
beyond its text: the bars, which move out of `shape.rs`, and the boxes. It
follows `layout/blocks.rs`, a builder that `shape_document` feeds line by
line. It holds the box's bounds, its checkbox range and its `checked` flag,
`DocumentLayout::task_at`, and the builder. A separate `tasks.rs` would leave
`line_bars` in `shape.rs` and leave nothing to pay for the new calls.

Line budget, counted on today's code:

- `shape.rs` loses `let mut bars` (line 112), `bars.extend(...)` (172),
  `bars,` (182) and `line_bars` with its doc (358-367) and one blank line: 14
  lines. It gains three: the builder's `new`, its `line` call, and its
  `finish` into the layout. Net −11. The split of the own conceals goes into
  `GutterCarry::layout` in `prefix.rs` (188 lines, unlisted, limit 400), so
  `line_prefix` keeps its length.
- `layout/mod.rs` swaps the `bars` field and its two doc lines (89-91) for one
  field holding both vectors, and `quote_bars` (158-161) reads through it.
  Net 0 or less.
- A listed file that shrinks must have its ceiling lowered in the same change:
  `./scripts/line-ceilings.sh --lower` (`scripts/line-ceilings.sh:58-87`).
  No ceiling rises.

### The hit test and the click

`task_at(local: Point<Pixels>) -> Option<(Range<ByteOffset>, bool)>` scans the
boxes, which sit only on lines whose task part is concealed. A revealed
checkbox is text, as D6 says. `on_mouse_down` gets `(layout, local)` from
`self.input.hit_test(position)` (`crates/app/src/text_input.rs:143-149`), as
`link_under` does (`mouse.rs:12-29`). It calls `task_at` after focusing and
before the `⌘`-click link branch (`mouse.rs:42-56`). A plain click on a box
toggles and returns before `self.is_selecting = true` (`mouse.rs:57`), so it
starts no drag selection. A modified click skips the box and keeps its
meaning (open question 1).

### The edit

The editor state gains a one-step replace beside `cut`
(`crates/editor/src/state.rs:147`): it calls `edit_ungrouped`
(`state.rs:78`), then collapses the selection to a given caret.
`edit_ungrouped` runs `Buffer::transact`, so the edit merges with the typing
on neither side, and is one undo step. Its `edited` collapses the caret to
`start + text.len()`, inside the checkbox. The method then sets the caret, as
a whole-line `paste` does (`state.rs:159-165`). `app` passes the middle byte's
range, the new byte (`x` for a space, a space for `x` or `X`), and the item's
text start. The edit keeps the byte count, so the text start does not shift.

### Theme

`colors.rs` gains `md_task_box` and `md_task_check` beside `md_quote_bar`.
Both take `md_quote_bar`'s values: `rgb(0x4a4a4a)` dark and `rgb(0xcecdc3)`
light (`builtin.rs:42,85`). The box then reads as the same quiet chrome as
the bar; a distinct colour waits for the scene. A vault theme file without
the keys keeps the built-in value: a file starts from its appearance's
built-in and overrides only the keys it holds (`crates/app/src/theme/file.rs:37`;
themes spec, "Errors"). The token count test
(`crates/app/src/theme/file/tests.rs:9`) goes from 30 to 32. `theme.rs` gains
`TASK_BOX_SIZE` beside `QUOTE_BAR_WIDTH` (`theme.rs:289`).

### Slices

Each slice is vertical and ends green on `./scripts/check.sh`, with the
`markdown-blocks` and `markdown-blocks-revealed` scenes rendered and looked
at. The scene already holds a task line (`crates/app/src/shot/scenes.rs:52`).
After each slice, run `cargo mutants` on the diff for `markdown` and
`editor`.

1. **S3a, the box shows** (~1 d). `Task`, `Gutter.task`, the marker decided
   at the pending slot (item 7), the task conceal, the task gutter
   (items 11, 13), the prefix in pieces, `marks.rs` with the box painted,
   both tokens and `TASK_BOX_SIZE`. No click.
   - `inkra-markdown`, by example: `- [ ] a`, `- [x] a`, `- [X] a`,
     `1. [ ] a`, `> - [ ] a`, a loose list `- [ ] a\n\n- [x] b`, `-    [ ] a`,
     a plain bullet next to a task keeps `•`; added: `- [ ]` at the end of
     the text is no task while `- [ ]\n` is one, an empty item `-\n` keeps
     its `•`, `- [ ]a` is no task, `- - [ ] a` keeps two `•` (item 13),
     `- a\n  - [ ] b` gives the inner line a gutter.
   - `inkra-markdown`, proptest: the conceal properties and the G-properties
     with item 12's change.
   - `app`: revealing a task marker moves no text; in `> - [ ] a` a caret on
     the `>` reveals the quote part only; a lazy line under `> - [ ] a`
     takes the quote part's width; a box paints while the caret is elsewhere.
2. **S3b, the click toggles** (~0.5 d). `task_at`, the one-step replace with
   its own `inkra-editor` tests (one undo step, merges with no neighbour, the
   caret set), the click in `mouse.rs`.
   - `app`: a click on a box toggles `[ ]` to `[x]` and back and moves the
     caret to the item's text start; `⌘Z` after a toggle restores the text;
     a click on a revealed checkbox places the caret and edits nothing; a
     toggle with the view scrolled away from the old caret does not scroll;
     added: a press on a box then a drag selects nothing; a `⌘`- or
     `⇧`-click on a box does not toggle; `[X]` toggles to `[ ]`.

The cut is not "data first". A `Gutter.task` with no reader is what As built
item 7 refused, and S3a reads it to paint. S3a also changes what `parse`
emits for every list item, so it ships with its scene, before any click
depends on it.

### Open questions

1. **Does a click with `⌘` or `⇧` held toggle?** Recommended: no, a plain
   click only. `⌘`-click keeps following links and `⇧`-click keeps extending
   the selection.
2. **Does `[X]` toggle to a space, and a space to lower-case `x`?**
   Recommended: yes, as D6 says.
3. **After `⌘Z` the caret sits inside the checkbox, so the box shows as text
   until the caret leaves.** Recommended: still accept it; the reveal rule
   says so. Not re-checked in the build: where `⌘Z` puts the caret.
4. **Does the box need a pointer cursor on hover?** No precedent exists in
   `app` (item 8). Recommended: defer it to the Deferred table, trigger "The
   user asks". A hover cursor is a new pattern that a link should get first.
5. **A task with no text (`- [ ]\n`).** Its text start is the line's end,
   which touches the checkbox, so a click reveals the marker as text.
   Recommended: accept; typing after it hides it again.
6. **`- [ ]` with no newline at the end of the note is no task** (Parser
   facts). Recommended: accept it as pulldown-cmark's behaviour; the box
   appears once a newline follows.
7. **A double click on a box.** Each press toggles, so a double click toggles
   twice. Recommended: accept; decision 1 says one click is one undo step.

### Deviations from Zed

- Zed's markdown preview is a separate pane. It renders a task as a
  `Checkbox` element in a slot one line tall
  (`vendor/zed-reference/crates/markdown/src/markdown.rs:2999-3032`). Inkra
  paints a quad over the checkbox's shaped width, in the gutter (Deviations
  from Zed, first item).
- Zed finds a task the same way: the item's next event, or the one after one
  `Start(Paragraph)` (`markdown.rs:3820-3836`). Inkra follows it (item 7).
- Zed's toggle writes all three bytes, `[x]` or `[ ]`, through `editor.edit`
  (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:1201-1229`).
  Its `debug_assert_eq!` expects lower-case `[x]` only. Inkra writes the
  middle byte, so `[X]` toggles too, and it moves the caret: the preview has
  no caret in the rendered text, the live editor has one.

## Deferred

| Item | Trigger |
|---|---|
| A checked task's text struck through or dimmed | The user asks. |
| A key to toggle the task at the caret (`⌘↵` follows a link) | Someone reaches for one. |
| The info string shown on a concealed code block | The user asks. |
| Highlighting inside fenced code | Unchanged from the live-preview spec. |
| A heading's `#` hanging inside a quote (live-preview spec, D12 limit) | Unchanged; the gutter makes it possible: the marker would hang at the gutter's edge. |
| A plain list item's hanging indent | A wrapped list item under a bullet is reported. The gutter supports it; only `Gutter`'s coverage changes. |
| Continuation lines of a task item align with its text | A multi-line task item is reported. |
| A quote inside a list item gets a gutter | A quote in a list is reported. |
| A tab before a nested `>` counts toward its indent | One is reported. |
