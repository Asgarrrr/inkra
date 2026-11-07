# Code blocks: concealed fences, a background, a language label

Status: **built 2026-11-07**. Supersedes S2 of
`2026-09-30-markdown-blocks-design.md`. That spec's S1 (quote gutters, #56) is
on `main` and is the base of this one.

## Why

A fenced block reads as source today. Its fences show dimmed, its text is
amber Menlo at the body size, and nothing tells it apart from the text around
it. Over several lines the amber is loud, and Menlo at 18 px looks larger than
the body.

This spec makes a block read as one calm object: fences hidden while the caret
is away, neutral code text, one rounded background, the language in a corner.

## Decisions taken

1. **Scope: rendering and the label.** Highlighting, a copy button, editing
   comforts and horizontal scroll are deferred (Deferred).
2. **A hidden fence keeps its row.** The fence line is an empty row inside the
   background. Nothing moves on reveal: one source line stays one displayed
   line, and the document's height is the same concealed and revealed. This
   follows the live-preview rule that reveal moves no text. The rows cost
   vertical space; the opening row holds the label.
3. **The whole block is one element.** A selection touching the block reveals
   both fences, through `is_active` unchanged. No second reveal rule.
4. **Code text is neutral and smaller.** A new `md_code_block` colour,
   default the body's soft grey, and 16 px Menlo in a 28 px row. Inline code
   keeps `md_code`. Highlighting will colour on top of `md_code_block`.
5. **Lines wrap at the column**, as today. The target is a horizontal scroll
   per block with a wrap setting; it gets its own spec (Deferred).
6. **Every block gets the look:** top level, inside a leading quote, inside a
   list item, and indented blocks. An indented block has no fence and no
   label.
7. **The label is always shown** while the opening fence is concealed. It is
   painted text with no state and no interaction.

## D1. The data

`inkra-markdown` gains one table, as `headings` and `gutters`:

```rust
pub struct Markdown {
    pub spans: Vec<Span>,
    pub conceals: Vec<Conceal>,
    pub headings: Vec<Heading>,
    pub gutters: Vec<Gutter>,
    /// Sorted by `range.start`.
    pub code_blocks: Vec<CodeBlock>,
}

pub struct CodeBlock {
    /// The whole block, fences included, up to its last line's end (no `\n`).
    pub range: Range<ByteOffset>,
    /// The info string's first word; `None` without one, and for an indented block.
    pub language: Option<Range<ByteOffset>>,
}
```

`language` holds a range, not a `String`: the layout reads the text from the
buffer, as it does for every span.

The info string's first word ends at the first ASCII whitespace. In
```` ```rust ignore ```` the language is `rust`.

pulldown-cmark gives each block's range whatever its container. Where an
indented block's range starts (before or after its four spaces) decides the
background's left edge (D4). The plan fixes it with a test before D4 is built.

## D2. Fence conceals

A fenced block is a concealable element, so the gap rule gives its fence
conceals:

| | `element` | `marker` | `replacement` |
|---|---|---|---|
| opening fence | the block's `range` | from the fence's first `` ` `` or `~` to the line's end, info string included | `""` |
| closing fence | the block's `range` | from the fence's first character to the line's end | `""` |

**The marker starts after the container prefix.** The gap rule splits each
gap piece that starts a line into its leading `[ \t>]*` run and the rest. The
run is a container prefix or an indent; only the rest is a marker. So the
closing fence's marker in `> ```` ` starts after the `> `, and never overlaps
S1's `>` conceal: `DisplayLine::new` asserts that markers never overlap. An
inline element's first gap piece starts with its own syntax, never a prefix
byte, so its conceals are unchanged. The prefix carries the parent's styles,
so a list item's indent stays in the text as body-font spaces in front of an
empty row.

An unclosed fence runs to the note's end, as CommonMark says. Only its
opening fence is concealed. Typing ```` ``` ```` turns the rest of the note
into code; the caret is inside, so nothing hides.

The live-preview invariants hold unchanged: every marker lies in one line,
markers never overlap, no active marker touches the selection.

## D3. The style

`Styles` gains `CODE_BLOCK`, bit 11. A code block's spans carry
`CODE | CODE_BLOCK`; its fences carry `CODE | CODE_BLOCK | MARKER`.

`CODE_BLOCK` adds to `CODE`, it does not replace it. A reader that tests
`CODE` (links skip code) stays correct with no change.

`markdown_style` sets `md_code_block` when `CODE_BLOCK` is present, and
`md_code` otherwise. `MARKER` still wins, so a revealed fence is dimmed.

## D4. The layout

**Size.** `line_metrics` is the one place that picks a line's
`(font_size, row_height)`. It takes the code block holding the line, found by
a cursor over `code_blocks` in the pass that walks `headings`. A block line
gets `(CODE_BLOCK_TEXT_SIZE, BODY_LINE_HEIGHT)`, 16 and 28 px. Rows keep their
height, so nothing shifts. A quote's gutter is still measured at the body
size by `GutterCarry`.

**Block geometry is built in `shape_document`, beside `bars`.** `paint` only
iterates it:

```rust
pub(crate) struct DocumentLayout {
    // …
    pub(crate) code_blocks: Vec<BlockLayout>,
}

pub(crate) struct BlockLayout {
    pub(crate) background: Bounds<Pixels>,
    pub(crate) label: Option<(Point<Pixels>, ShapedLine)>,
}
```

- **Top and bottom.** From the first line's `top` to the last line's bottom,
  every wrapped row included.
- **Left.** The `x` where the block's first byte stands on its first line,
  read from that line's shape, minus `CODE_BLOCK_OUTSET` (8 px). At the top
  level this is the column's edge. In a quote it is past the gutter, so the
  bars stay visible beside the block. In a list it is the fence's column.
  None of the three needs its own code.
- **Right.** The text column's right edge plus `CODE_BLOCK_OUTSET`.
- **Corners.** `CODE_BLOCK_RADIUS`, 6 px.

Blocks and lines are both in document order: one cursor walks both. Finding
each block's lines by `line_for_offset` would scan the lines once per block.

**The label** exists when the block has a `language` and its opening fence is
concealed. It is shaped in the UI font at 12 px, `text_muted`. Each distinct
label text is shaped once per `shape_document` call, as `GutterCarry` measures
each distinct gutter text once; most notes use a handful of languages. It
stands right-aligned, `CODE_BLOCK_OUTSET` inside the background's right edge,
centred in the opening row.

**Wrap.** A block line wraps at the column width, as today. `wrap` is already
computed per line (`shape.rs`) and `shape_text` takes `None` for no wrap: the
future setting passes `None` to the lines the cursor already knows are block
lines. Nothing is added for it now.

**Cost.** One more cursor in the existing pass: O(lines + blocks). No
keystroke scans the document again. Reveal changes neither height nor wrap,
except a revealed info string longer than the column, which adds a row.
Accepted.

## D5. Paint order

The background goes under the selection. The selection paints in
`text_input/paint.rs` before `layout.paint`, so a separate
`DocumentLayout::paint_backgrounds` runs before the selection quads. Order:

1. code block backgrounds;
2. the selection;
3. quote bars;
4. the text and the revealed prefixes;
5. labels;
6. the caret.

Only visible blocks paint, filtered as the bars are.

## D6. Theme

Two tokens join `tokens!`, each with a dark and a light value in
`builtin.rs`:

| Token | Dark | Light |
|---|---|---|
| `md_code_block` | `#cecdc3` | `#403e3c` |
| `md_code_block_background` | `#1c1b1a` | `#f2f0e5` |

The label uses `text_muted`. `CODE_BLOCK_TEXT_SIZE`, `CODE_BLOCK_OUTSET`,
`CODE_BLOCK_RADIUS` and the label size live in `theme.rs`. A vault theme sets
both tokens with no change to `theme/file.rs`. The token count test moves
from 28 to 30.

## Edge cases

| Case | Behaviour |
|---|---|
| ```` ``` ```` with no info string | Background, no label |
| Empty block (two fences) | Two empty rows in the background; a label if a language is given |
| `~~~` and fences longer than three | Same as ```` ``` ```` |
| Click on a concealed fence row | Lands on that line, which touches the block, so the fences show |
| A selection over part of the block | Touches it, so the fences show; height unchanged |
| A block in a list in a quote | Gutter, then the list indent, then the background at the fence's column. A test and the scene check it; if it breaks, it moves to Deferred |
| Blank lines inside a block | Rows inside the background |

## Deviations from Zed

- **Quads, not boxes.** Zed renders its markdown preview's blocks as styled
  `div`s with padding, margins, a border and an optional horizontal scroll
  (`vendor/zed-reference/crates/markdown/src/markdown.rs`). An editor needs one
  displayed row per source line, so we paint quads and the fence rows give
  the vertical padding.
- **A language label.** Zed shows none; it uses the language for
  highlighting only.
- **No copy button.** Zed shows one on hover. Deferred.

## As built

- **Fence conceals come from the gap rule, not an explicit emission (D2).**
  The gap rule now skips a line-start container prefix in every gap piece,
  not only after a `\n` in the gap. A closing fence's marker then starts after
  its prefix with no special case, and one conceal path serves every element.
- **A block's line-start indents use the body font, not Menlo (D2).** They
  carry the parent's styles, not the block's. The opening line's indent lies
  outside the block and shapes in the body font, so the indents of every line
  align with it.
- **Paint order is checked by the `code-blocks-revealed` scene.** gpui's scene
  order cannot be observed headlessly.
- **With no column, the right edge is the widest block line plus the outset.**
- **The label shows the info word's source text, escapes unresolved (D1).**
  `CodeBlock.language` is a range of the opening line, not pulldown-cmark's
  unescaped info string, so `a\_b` labels as `a\_b`.
- **`markdown_parse` stays within noise.** Three interleaved rounds against `docs/code-blocks`
  (0c4878c): 5 KB 26.2–26.4 µs vs 26.5–27.7 µs, 50 KB 242–247 µs vs 243–248 µs, 500 KB
  2.38–2.43 ms vs 2.39–2.40 ms. A single `--quick` run had read +14%.
- **Spec slice C1 shipped as two slices (Slices).** The style came first, then the
  conceals.

## Tests

- `inkra-markdown`, `#[test]`:
  - `code_blocks` for backticks, `~~~`, a long fence, no info, a multi-word
    info (`rust` kept), unclosed, empty, indented, in a quote, in a list, in a
    list in a quote;
  - fence conceals: `element` is the block, `marker` starts after the
    container prefix;
  - block spans carry `CODE | CODE_BLOCK`, inline code `CODE` only.
- `inkra-markdown`, `proptest`: the conceal invariants extend to fences
  (one line each, no overlap); block ranges are sorted and inside the text.
- `app`, `#[gpui::test]`:
  - a block line shapes at 16 px in 28 px rows;
  - a caret in the block reveals both fences, and the document height is
    equal before and after;
  - one `BlockLayout` per block, from its first line's top to its last row's
    bottom; the left edge follows the gutter and the fence's column;
  - a label only while the fences are concealed and a language is given;
  - backgrounds paint before the selection.
- Scenes `code-blocks` and `code-blocks-revealed`, dark and light: a
  top-level block with a line that wraps, one in a quote, one in a list, one
  with no language, one indented. Look at each after every slice.
- `cargo mutants` on `inkra-markdown`: zero missed.

**Measure.** `markdown_parse` stays within noise, and so does the keystroke
median on a note dense with blocks, in interleaved rounds against base, as S1
measured.

## Slices

Each slice is vertical: data, layout, paint, tests, scene.

1. **C1, fences and style.** `CodeBlock`, `CODE_BLOCK`, fence conceals,
   `md_code_block`, 16 px in `line_metrics`, the scenes. A block already reads
   calm, with no background.
2. **C2, the background.** `BlockLayout.background`,
   `md_code_block_background`, `paint_backgrounds` before the selection;
   quotes, lists and indented blocks.
3. **C3, the label.** Shaped once per distinct text, placed, hidden on reveal.

## Deferred

| Item | Trigger |
|---|---|
| Horizontal scroll per block, a wrap setting | The stated target. Its own spec when started. |
| Highlighting inside a block | The next step asked for; tree-sitter or syntect is that spec's choice. |
| A copy button on hover | Asked for. It needs a per-block hover state the editor lacks. |
| Editing comforts: Tab indents, a typed fence closes, Enter keeps indent | Asked for. |
| A wrapped block line hangs at its indent | Before no-wrap ships, if a wrapped block reads badly. |
