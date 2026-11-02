# Markdown blocks: quote bars, code backgrounds, task boxes

Status: **S1 built 2026-10-01** on `feat/markdown-blocks-s1`; S2 and S3
pending.

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
