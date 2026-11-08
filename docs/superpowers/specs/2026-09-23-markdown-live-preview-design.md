# Markdown live preview

Status: approved (2026-09-23). Slices 1 (styled source), 2 (concealment) and
3 (heading sizes) implemented. Slice 5 (D12, hanging heading markers)
implemented on 2026-09-24. The first draft was a source-mode design; "What the review
changed" records what survives from it.

The editor has one mode. A note reads as formatted text: `**strong**` shows as
**strong**, a heading shows larger without its `#`, a list item shows a `•`.
Syntax reappears only on the element being edited. When the caret or the
selection touches `**strong**`, its `**` reappear, dimmed. When the caret
leaves, they hide again. Everything else stays formatted.

This is the first row of "Parse markdown; inline styling" in
`2026-09-20-modular-architecture-design.md`, and it creates `inkra-markdown`.
That spec also said live preview needs virtualization, because `uniform_list`
places rows at `item_height * ix`. That no longer holds: since soft wrap, rows
are our own table in `DocumentLayout` (`crates/app/src/editor/layout/mod.rs:70`),
and gpui shapes and paints one line per call with its own font size and line
height (`gpui/src/text_system.rs:750`, `text_system/line.rs:460`).

## Behaviour

- **Reveal unit: the element.** An element is revealed when its range
  intersects the selection, ends included. A caret at `|**x**` or `**x**|`
  reveals it. An element is a heading, an emphasis, a strong, a
  strikethrough, an inline code span, a link, a wikilink, an autolink, or a
  list bullet.
- **Nesting reveals the chain.** The caret in `*it*` inside `**bold *it***`
  reveals both.
- **A selection reveals every element it touches.** No concealed byte is ever
  inside the selection, so a delete or a copy never removes or copies syntax
  the user cannot see.
- **Headings are larger**, per level, whether revealed or not. Revealing a
  heading shows its `# ` in the left margin, without moving its text (D12).
- **A bullet `-`, `*` or `+` shows as `•`.** Numbers stay as typed.
- **A link shows its text only.** `[t](u)` shows `t`, `[[page|alias]]` shows
  `alias`, `[[page]]` shows `page`, `<u>` shows `u`, in the link colour.
- **Everything else shows its syntax, dimmed**, as in source mode: images,
  quotes, code fences, task boxes, frontmatter. Concealing those is deferred.
- **Layout shifts when an element reveals.** Revealing `**` pushes the text
  after it right. Obsidian and Typora accept the same shift. A heading's
  opening `# ` is the exception: it hangs in the margin (D12).

## What Zed does

Read in `vendor/zed-reference/`:

- **Parsing.** Zed uses tree-sitter-md as two grammars. The block grammar
  injects the inline grammar into every `inline` node
  (`crates/grammars/src/markdown/injections.scm:6-10`). The editor keeps every
  marker visible, styled as punctuation.
- **Reparse.** The reparse is incremental, first synchronous with a 1 ms
  budget (`crates/language/src/buffer.rs:1166-1170`, `:1949`), then on a
  background task.
- **Style.** A capture resolves to a `HighlightStyle`, then to a `TextRun`
  (`crates/editor/src/element.rs:7574-7592`). Neither carries a size, and the
  editor never enlarges a heading. The rendered preview is a separate view
  (`crates/markdown`), with pulldown-cmark 0.13 and `into_offset_iter`
  (`crates/markdown/src/parser.rs:258-266`).
- **Hiding text.** Zed hides and replaces text in its `DisplayMap`, a stack of
  layers (`crates/editor/src/display_map.rs:1-40`). `FoldMap` hides a range
  behind a `FoldPlaceholder` (`display_map/fold_map.rs:27`). Each layer keeps
  a sum tree of transforms between input and output coordinates.
- **Fork.** Zed builds tree-sitter-md from its own fork, which fixes a buffer
  overflow in `serialize()` (`Cargo.toml:889`, upstream
  tree-sitter-grammars/tree-sitter-markdown#243, still open on 2026-09-23).

Zed has no single mode that mixes preview and source in one editor. The
reveal rule below has no Zed reference; it follows Obsidian's live preview.

## Deviations from Zed

### X1. pulldown-cmark, not tree-sitter-md

Measured on an Apple M5 Pro, release build, pulldown-cmark 0.13.4 and
tree-sitter-md 0.5.3. The corpus is real files from `docs/`, concatenated. The
bench source is kept out of the repo.

| Note | pulldown-cmark, full parse + collect | tree-sitter-md block grammar only, incremental 1-char edit | tree-sitter-md `MarkdownParser`, incremental |
|---|---:|---:|---:|
| 5 KB | 17 µs | 322 µs | 694 µs |
| 50 KB | 89 µs | 2 093 µs | 8 003 µs |
| 500 KB | 954 µs | 25 717 µs | 90 375 µs |
| 50 KB, one dense paragraph | 176 µs | 4 414 µs | 41 050 µs |

Values are medians. `MarkdownParser` re-parses every inline node on every
call, so its column overstates a Zed-style pipeline. The "block grammar only"
column is a lower bound for one: it excludes every inline parse, and it is
still about 20 times slower than a full pulldown-cmark parse. The cost is the
block grammar's external scanner, which a syntax map would not remove.

The other reasons agree with the numbers:

- tree-sitter-md's README advises against it "where correctness is
  important". pulldown-cmark targets CommonMark.
- The `serialize()` overflow is open upstream. We would have to vendor a fork,
  and `deny.toml` denies unknown git sources.
- tree-sitter-md compiles C through `cc`, and enables wikilinks only through a
  build-time environment variable. pulldown-cmark is pure Rust, with three
  direct dependencies, and `ENABLE_WIKILINKS` is a runtime option (0.13.0 and
  later).

`comrak` has 27 direct dependencies and line/column positions. `markdown`
(wooorm) has had no release since 2025-04-23 and has no wikilinks. Both are
rejected.

### X2. A full reparse per edit, synchronous, with no background fallback

A full parse of a 50 KB note costs 89 µs, well under 1 ms. One synchronous
parse per edit keeps one code path and never paints unstyled text. The
trigger that reverses this is in Deferred.

### X3. A per-line concealment map, not a layered `DisplayMap`

Zed's layers exist for folds across lines, inlays, tabs and blocks, over
multi-buffers, with sum trees for edits in large files. Here, every concealed
range lies inside one line (D4), and the concealment depends on the selection,
so it changes on caret moves, not only on edits. Each line therefore builds
its own small map at shape time, from the concealments active on it. No
concealment crosses a line, so no row or line count changes.

### X4. No highlighting inside fenced code

Zed injects the fence's language into a code block. We style a code block as
one monospace span. A note application is not an IDE.

### X5. A heading's opening marker hangs in a margin

Zed's editor never hides a heading marker, so it has no reference for where a
revealed one goes. Its gutter holds line numbers and fold controls, painted
beside the text, never text of the buffer. D12 paints buffer text in a margin
left of the column. The user chose it from a mock-up on 2026-09-23. In a pane
too narrow for the margin, a revealed marker shows inline instead, as Obsidian
does everywhere (D13).

## Decisions

### D1. `inkra-markdown`: `&str` in, spans and concealments out

A new crate between `text` and `app`. It depends on `inkra-text` (for
`ByteOffset`) and on `pulldown-cmark` with `default-features = false`. It has
no gpui, filesystem or clock. In the same change it joins: `members` and
`[workspace.dependencies]` in `Cargo.toml`; the fmt list and the no-gpui list
in `scripts/check.sh`; the fmt command, the Architecture section, "Where code
goes" and the `cargo mutants` line in `CLAUDE.md`.

```rust
pub fn parse(text: &str) -> Markdown;

pub struct Markdown {
    pub spans: Vec<Span>,         // sorted, non-overlapping
    pub conceals: Vec<Conceal>,   // sorted by marker start
}

pub struct Span {
    pub range: Range<ByteOffset>,
    pub styles: Styles,
}

pub struct Conceal {
    pub element: Range<ByteOffset>, // what reveals it
    pub marker: Range<ByteOffset>,  // what hides; inside `element`, inside one line
    pub replacement: &'static str,  // `""` hides, `"•"` replaces a bullet
}
```

`Styles` is a small `Copy` bitset of semantic tags: `Heading(level)`,
`Strong`, `Emphasis`, `Strikethrough`, `Code`, `Link`, `Marker`, `Quote`,
`Frontmatter`. A bitset, because clippy pedantic rejects a struct of more than
three bools (`struct_excessive_bools`). It carries no colour, font or size.

Spans are flattened: `**bold *it***` gives `Strong` on `bold `,
`Strong | Emphasis` on `it`, and `Strong | Marker` on the `**`. Adjacent spans
with equal `Styles` are merged. Bytes with no style get no span.

The parser options are `ENABLE_STRIKETHROUGH`, `ENABLE_TASKLISTS`,
`ENABLE_WIKILINKS`, `ENABLE_YAML_STYLE_METADATA_BLOCKS` and `ENABLE_TABLES`.
Not `ENABLE_SMART_PUNCTUATION`: it rewrites text, and we only read ranges.

### D2. What each construct does

| Construct | Content | Markers (D3) | Concealed when not revealed |
|---|---|---|---|
| ATX heading | `Heading(n)` | `#…# `, closing ` ##` | the opening `#…# ` only, hung in the margin (D12); element = the heading. The closing ` ##` is dimmed, never hidden |
| setext heading | `Heading(n)` | the `===` / `---` line | no, dimmed |
| `**x**`, `__x__` | `Strong` | the delimiters | yes; element = the strong |
| `*x*`, `_x_` | `Emphasis` | the delimiters | yes |
| `~~x~~` | `Strikethrough` | the delimiters | yes |
| `` `x` `` | `Code` | the backtick runs | yes |
| list bullet `-`, `*`, `+` | — | the bullet | replaced by `•`; element = the bullet itself |
| ordered list number | — | the number and its `.` or `)` | no, dimmed |
| fenced / indented code block | `Code` | fence lines, info string included | no, dimmed |
| `[t](u)`, `[t][r]`, `[r]`, `<u>` | `Link` on the text | `[`, `](u)`, `][r]`, `<`, `>` | yes; element = the link |
| `![alt](u)` | `Link` on the alt text | `![`, `](u)` | no, dimmed |
| `[[target\|alias]]` | `Link` on the alias, or on the target | `[[`, `target\|`, `]]` | yes; element = the wikilink |
| `> quote` | `Quote` | `> ` on every line | no, dimmed |
| task `[ ]`, `[x]` | — | the box | no, dimmed |
| YAML frontmatter | `Frontmatter` | the `---` fences | no, dimmed |
| thematic break | — | the whole line | no, dimmed |

A bullet's element is the bullet character alone — `-`, not `- ` and not the
item — and only that character is replaced; the space after it stays
displayed. With `- x`, the element is 0..1 and the content starts at 2, so a
caret at the content's start, where Home lands, does not touch the bullet. A
wider element would reveal `-` whenever the user types at the start of an
item, and shift the line. Only offsets 0 and 1 reveal it.

Block elements end before their line ending. pulldown-cmark's ATX heading
range includes the trailing `\n` (`firstpass.rs:1620,1646`, `:797-799`), so
`# T\nx` would give the heading 0..4, and a caret at column 0 of the next line
would reveal `# `. `parse.rs` clips every element range to exclude a trailing
`\n`; a `\n` byte never carries a span or a marker.

A table is parsed, so a `|` row is not taken for a paragraph. By D3 its pipes
and its delimiter row are markers and render dimmed; it gets no other style. A link reference definition produces no event, so it renders as body
text.

### D3. Markers are the bytes that no leaf event covers

pulldown-cmark reports an element's start and end with one range, the whole
element. Leaf events report the text inside. **A byte inside at least one
element, and covered by no leaf, is a marker.** It takes the tags of its
innermost enclosing elements, plus `Marker`. The leaves are `Text`, `Html`,
`InlineHtml`, and the content of `Code`. One pass over the event stream, with
a stack of open elements, computes it.

The rule is global, not per container, because of continuation lines. In
`> a\n> b\n` the paragraph spans 2..8 and contains the second `> ` (4..6), but
no child covers it: `SoftBreak` is 3..4 and `b` is 6..7. Checked on 0.13.4.

Three events are read for themselves. Inline code is one `Code` event with
its backticks, so its backtick runs are markers. `TaskListMarker` and `Rule`
are markers. `SoftBreak` and `HardBreak` are markers.

A `Conceal` is emitted per concealable marker run from D2, with its element's
range. An emphasis delimiter is one line by construction; a heading's markers
are on its line. A proptest holds this: every `marker` lies in one line.

### D4. The reveal rule is pure and lives in `inkra-markdown`

```rust
pub fn is_active(conceal: &Conceal, selection: &Range<ByteOffset>) -> bool;
pub fn touched<'a>(conceals: &'a [Conceal], selection: &'a Range<ByteOffset>)
    -> impl Iterator<Item = &'a Range<ByteOffset>>;
```

A conceal is active when its `element` does not intersect the closed interval
`[selection.start, selection.end]`. So the invariant, held by a proptest:
**no active marker contains or touches the selection.** The caret is always on
displayed text. Every existing caret, IME and editing rule keeps working
unchanged, because it only ever sees revealed text around the caret.

### D5. `DisplayLine`: the text a line shows, and the offsets both ways

Also in `inkra-markdown`, since it is pure:

```rust
pub struct DisplayOffset(pub usize);

impl DisplayLine {
    pub fn new(line: Range<ByteOffset>, hidden: impl IntoIterator<Item = (Range<ByteOffset>, &'static str)>) -> Self;
    pub fn text<'a>(&self, line: &'a str) -> Cow<'a, str>;
    pub fn to_display(&self, offset: ByteOffset) -> DisplayOffset;
    pub fn to_buffer(&self, index: DisplayOffset, bias: Bias) -> ByteOffset;
}
```

`DisplayOffset` is a UTF-8 byte index into the display text — what gpui's
`x_for_index` and `closest_index_for_x` take (`gpui/src/text_system/line_layout.rs:108`).
It is a third offset kind, so it gets a newtype, like `ByteOffset` and
`Utf16Offset`; CLAUDE.md's "never mix them" extends to it.

A `DisplayLine` holds the line's range and its active hidden ranges, not its
text. `text` borrows the line when nothing is hidden, so `shape.rs` still
copies each line into a `SharedString` once, as `layout/shape.rs:40` does
today. A line with nothing hidden maps offsets by subtraction.

Both maps are defined everywhere and monotonic:

- `to_display` of an offset inside a concealed marker clamps to the display
  index of the gap. `bounds_for_range` (`crates/app/src/editor/input.rs:80-96`)
  receives arbitrary ranges from AppKit, not only the caret's.
- `to_buffer(d, Bias::Left)` is the smallest buffer offset that maps to `d`
  and is not strictly inside a hidden range; `Bias::Right` is the largest. In
  `**a***b*` with both concealed, buffer 3 and 6 both display at 1:
  `to_buffer(1, Left)` is 3 and `to_buffer(1, Right)` is 6. `Bias` is
  `inkra_text::Bias`.

Who uses which bias:

- A row's start, a click and a vertical move use `Left`. A click on `|bold`
  lands before the `**`, at the element's start, which touches it and reveals
  it.
- The last row of a line ends at the line's end in buffer offsets, not
  through the map. `End`, or a click past the end of a line, therefore lands
  after a trailing hidden `**`, touches the element and reveals it. With
  `Left` the caret would stop before the `**`, and `End` would need two
  presses.
- An inner row's `caret_end` uses `Left`, so it stays before the next row's
  start, which D4 of the soft-wrap spec requires.

### D6. `DocumentLayout` shapes display text

`shape_document` takes the `Markdown` and the selection. Per line it finds the
line's spans and conceals with a cursor that advances with the lines — both
lists are sorted, and every marker lies in one line — so the whole document is
one pass over each list, not a scan per line. It filters the active conceals,
builds the `DisplayLine`, and shapes its text. `shape_document` runs once per
measure probe, up to twice per frame (`element.rs:72-109`), so this matters.
gpui's cache keys on text content, so a display string rebuilt each frame
still hits (`text_system/line_layout.rs:614-620`). `LineLayout`
keeps the `DisplayLine`. Every offset↔pixel conversion already goes through
`DocumentLayout` (`layout/mod.rs:38-39`); each now converts through the line's
map first: `x_on_row`, hit-testing, row boundaries from `wrap_boundaries`,
and selection quads.

Rows snap to graphemes of the display text, then map to buffer offsets. The
soft-wrap rules are otherwise unchanged.

`NameField` (`crates/app/src/field.rs:357`) passes an empty `Markdown`. A file
name is not markdown.

### D7. The reveal follows the selection, so a move can reshape

The active set depends on the selection. `shape_document` already runs every
frame in `request_layout` (`element.rs:99`), so the painted text follows the
caret without new code. But `reveal_cursor` (`mod.rs:150`) and
`move_vertically` (`mod.rs:170`) read `last_layout`, which only `reshape`
refreshes, at edit time. After a move that reveals an element, both would
work from a stale layout.

So `move_cursor` (`mod.rs:142`) reshapes when the active set changed, before
`reveal_cursor`. `update_state` only sets the marked range, and an IME edit
goes through `edit_buffer`, which reshapes anyway. `reshape` needs the
`Window`, so `move_cursor` gains a `window` parameter; its callers are action
handlers and mouse listeners, which have one. They compare the element
ranges that contain or touch the old and the new selection, through
`touched`. `touched` scans every conceal, so a caret move costs O(conceals),
twice. A move inside one element, or across plain text, reshapes nothing.

### D8. Headings are taller: rows get their own height

`theme` holds a size and a line height per heading level. A line whose text
carries `Heading(n)` is shaped at that size, and all its rows take that line
height. A setext heading's text lines take it; its underline line does not.

`DocumentLayout` stores a top and a height per line, the tops a prefix sum
built during `shape_document`. `top_for_row` becomes the line's top plus the
row's index within the line times the line's height. `row_for_y` becomes a
binary search over line tops. Every use of the single `line_height` moves to
the row's or line's own height:

| Use | Where |
|---|---|
| row tops | `top_for_row`, `layout/mod.rs:70-74` |
| hit-testing y | `row_for_y`, `layout/mod.rs:109` |
| IME rectangle | `range_bounds`, `layout/mod.rs:180` |
| newline stub width | `selection_quads`, `layout/mod.rs:201` |
| selection quad height | `selection_quads`, `layout/mod.rs:207` |
| row spacing when painting a wrapped line | `paint`, `layout/mod.rs:219-221` |
| caret quad | `element.rs:144` |
| reveal margin | `reveal_cursor`, `mod.rs:161` |
| `NameField` caret and IME rectangle | `field.rs:279`, `:358-371` |

`line_height()` becomes `row_height(row)`. `NameField` is one body line and
calls `row_height(0)`. Missing the `paint` row would paint a wrapped H1's
second row at body spacing while `top_for_row` reserves heading spacing.

The column stays 70 `ch` of the body font. A heading line wraps earlier; its
glyphs are wider.

### D9. `app` maps `Styles` to a `TextStyle` in `theme.rs`

`theme::markdown_style(base: &TextStyle, styles: Styles) -> TextStyle`
applies, in a fixed order:

- `Heading(n)`: bold, `theme::heading_text()`. The size is the line's (D8).
- `Strong`: bold. `Emphasis`: italic. `Strikethrough`: strikethrough.
- `Code`: `font_family = theme::CODE_FONT_FAMILY`, set on the `TextStyle`
  directly, because `HighlightStyle` has no family field
  (`gpui/src/style.rs:580-601`). Colour `theme::code_text()`.
- `Link`: `theme::link_text()`. `Quote`: `theme::quote_text()`.
- `Frontmatter`: `theme::marker_text()`.
- `Marker` last: `theme::marker_text()`. It keeps weight and style, so a
  revealed `**` inside a heading is bold, and marker widths match their text.

`CODE_FONT_FAMILY` is `"Menlo"`, which ships with macOS. The application runs
only on macOS today; a second platform makes it per-platform.

`line_runs` (`layout/runs.rs:51`) merges, over the display text, the spans mapped
through the line's map and the IME marked range. The marked range is always
revealed text (D4). A composition inside `**bold**` keeps the bold and gains
the underline. `shape_document` resolves each distinct `Styles` value to a
`TextStyle` once per call, not once per run.

**Any style boundary is a cache key.** gpui's line cache keys on the line's
font runs (`text_system/line_layout.rs:614-620`), and `shape_text` starts a
new font run whenever the decoration changes, colour included
(`text_system.rs:797-808`). So a line is laid out again whenever its display
text or its style boundaries change: the edited line, the lines of an element
that reveals or hides, and every line below a fence being typed.

### D10. `Markdown` is parsed once per text change, not once per frame

`EditorView` keeps `markdown: Option<(u64, Markdown)>`, tagged with the
`Buffer::version()` it was parsed from. `shape_document` never parses. One
method, `refresh_markdown`, re-parses when the tag is `None` or differs from
the buffer's version. It is called:

- in `EditorView::new` and in `open_buffer`, which receive a note that already
  has text. `open_buffer` resets the tag first: versions are per buffer, and
  `NoteStore` keeps buffers open (`crates/app/src/vault/store.rs:94-96`).
- in `reshape`, before shaping. The buffer observer runs only when effects
  flush, after `edit_buffer` has already reshaped.
- in the buffer observers (`mod.rs:54`, `:77`). They cover a write outside the
  gates; tests do it today (`crates/app/src/shell/tests/renaming.rs:57-63`).

`version()` survives undo: it is the id of the last edit left on the undo
stack (`crates/text/src/buffer.rs:84-89`). `line_runs` and `display_line`
clip every range to their line, so a stale `Markdown` mis-renders one frame
and cannot panic. A stale heading range spans lines, so for that frame it
mis-sizes every line it still covers.

### D11. Modules: each brick has one job and one direction

`inkra-markdown`, one concern per file, tests in their own files:

| Module | Job | Knows about |
|---|---|---|
| `styles.rs` | `Styles`, the tag bitset | nothing |
| `parse.rs` | `parse`: pulldown-cmark events → `Markdown` (D1–D3) | `styles` |
| `reveal.rs` | `Conceal`, `is_active`, `touched`: the reveal rule (D4) | ranges only |
| `display.rs` | `DisplayLine`: display text and both offset maps (D5) | ranges only |

pulldown-cmark stays inside `parse.rs`; no type of it crosses the crate
boundary. Replacing the parser changes one file. `reveal` and `display` do
not know markdown exists: they take ranges. A second source of concealment —
a folded block, a rendered wikilink — reuses them without change.

`inkra-app`: before slice 1, `layout.rs` was split into `editor/layout/`, as a
pure move checked with `./scripts/test-names.sh`:

| Module | Job |
|---|---|
| `mod.rs` | `DocumentLayout`: rows, tops, every offset↔pixel conversion |
| `shape.rs` | `shape_document`: per line, conceal → display text → runs → `shape_text` |
| `runs.rs` | `line_runs`: spans and the marked range merged into `TextRun`s |
| `rows.rs` | `rows`: wrap boundaries → grapheme-snapped rows |

`theme.rs` gains `markdown_style` and the markdown colours and sizes, and
nothing else. `EditorView` holds the parse (D10). Markdown semantics in `app`
stay in `theme::markdown_style` and `layout/shape.rs`; `layout/{mod,rows,runs}.rs`
and `field.rs` see only `DisplayLine`, `DisplayOffset` and `Markdown` as data. Views still talk through events; nothing here adds a reference
between views.

### D12. A heading's opening marker hangs in the margin

**Problem.** Slice 3 concealed `# ` like `**`: out of the display text when
not revealed, back in when revealed. Putting it back re-wraps the line. The
text moves right, and a heading that nearly fills the column gains a row, so
everything below it drops by one heading row. `# aaaa aaaa aaaa aaaa aaaa aaaa
aaaa a\nbody` lays out 2 rows concealed and 3 revealed.

**Rule.** A heading's shaped text never depends on the reveal state. Its
opening marker is never part of it. When the heading is revealed, the marker
paints in a margin left of the column, right-aligned against it. The closing
sequence stays in the text, dimmed, in both states. No reveal of a heading can
move text, by construction.

**The parser names the opening marker.** `Heading` gains
`marker: Option<Range<ByteOffset>>`: an ATX heading's opening sequence, its
`#` run and the spaces or tabs after it, or `None` for a setext heading.
`parse.rs` conceals it with one explicit call at `Tag::Heading`, as `bullet`
does for a bullet. The same range feeds `marker` and the `Conceal`, so the two
cannot disagree. A heading's `Open` gets no `element`, so the gap rule (D3)
still dims both sequences but conceals neither. The closing sequence is
therefore dimmed and never hidden, and so is any other gap directly inside a
heading: the `\` of `# a\*b` now shows dimmed, as it does in a paragraph. This names a structure, not a place, so
the crate stays free of pixels.

Deriving the marker in `app` was rejected. "The first conceal on a heading
line, at the line's start" also matches the `**` of the setext heading
`**b**\n===`, whose strong element starts where the heading starts.

**The margin is only at the line start.** A marker hangs when
`marker.start` is its line's start. `> # q`, `- # a` and `  # q` keep slice
3's inline conceal, and keep its re-wrap: no margin exists left of a
container prefix. This is an accepted limit.

**Layout.** In `shape_document`, a hanging marker goes to `DisplayLine` as a
hidden range in both states. It is left out of the reveal filter. The line
also gets a `Hanging { range, shaped: Option<ShapedLine> }`:

- `LineLayout.hanging: Option<Hanging>` is `Some` whenever the marker hangs,
  revealed or not: the coordinates below need its range in both states.
- `shaped` is `Some` only while the marker's `Conceal` is not active (D4). A
  concealed marker needs no shape; its width is the shape's own.
- The marker shapes through `line_runs` over its own range with an identity
  `DisplayLine`, at the heading's size. Its style (heading weight, `Marker`
  dim) and an IME underline come from the same code as the body's. There is
  no second styling path.
- The marker's trailing space is its gap to the text. It is right-aligned, so
  its end is at `x = 0`, where the body starts. The offset at `marker.end` has
  one `x` in both, and the caret there needs no tie-break.

**Coordinates.** Only two functions branch on `hanging`, and only on row 0:

- `x_on_row`: an offset before `marker.end` gets `Hanging::x_for_offset`, a
  negative `x`. `caret_bounds`, `range_bounds` (IME) and `selection_quads`
  call it and need no change of their own.
- `offset_on_row`: `x < 0` hit-tests the marker. `position_for_point` already
  passes a negative `x` (`editor/mod.rs:299`).

`selection_quads` draws a row that is not the first one from the `x` of the
row's start, not from `px(0.)`. On a revealed heading that start is the
marker's, so a selection across a heading highlights its `#` in the margin.
The selection revealed it, and its bytes are selected. `paint` draws
`hanging.shaped` at `x = -width` on the line's top, with the line's height, so
the baselines match. `rows()` is unchanged: a hidden prefix already maps row
0's start to `line.start` (`display.rs`, `to_buffer` with `Bias::Left`).

**The body never maps into the marker.** A click or a vertical move at
`x >= 0` on a line that hangs its marker lands at `marker.end` or later, shown
or not. D5's `Bias::Left` would put it at `line.start`, left of the `#`, away
from the click. D5 lands `|bold` before the `**` so that the click touches the
element; a heading's element is its whole line, so any offset on it already
touches it. The marker's own offsets are reached through the margin, `x < 0`,
or by the arrow keys. Row 0 of a hanging line splits at `x = 0` and at nothing
else. A concealed marker shows no glyph, so its whole margin lands on
`line.start`, which reveals it: a goal `x` left in the margin by `Home` comes
back to the `#` after a vertical move away and back. `Home` goes to `line.start`, left of the `#` in the
margin. An empty heading `#` follows the same rule: `x >= 0` lands at
`marker.end`.

**The margin is measured, not guessed.** D13 replaces this paragraph's fixed
margin with one that shrinks to a floor. `TextElement` measures the margin
where it measures the column (`element.rs`, `column_width`): the widest
opening marker, `#` × n and a space, shaped at level n's size and weight, over
n = 1 to 6, rounded up to a whole pixel so taffy's snapping does not shift
the text. Markers hang on the left only, but the right side mirrors the margin. Before
D13 the text started at `left = max(margin, (pane − column) / 2)` and was
`min(column, pane − 2 × margin)` wide, never below zero. A wide window keeps the
column centred where it was. A narrow one gives up both margins from the
column's width, so the text stays centred. The first version reserved the left
margin only, to save 74 px of text in a narrow pane; it left the text visibly off
centre beside the docks, and the user chose centring on 2026-11-08. A pane
narrower than both margins leaves the text zero wide,
one glyph per row (`text_system/line_layout.rs:251`). The editor's `p_4()`
stays. `NameField` shapes with `Markdown::default()` and gets
no margin. A marker with extra spaces, `#   T`, can be wider than the margin;
before D13 it painted into the pane's padding, and D13 shows it inline instead.

The marker and the body share a baseline only when they share metrics. A
heading holding inline code takes Menlo's larger descent, so its body sits
0.42 px (one device pixel) above its marker. gpui places a line's baseline
from that line's own ascent and descent (`text_system/line.rs:547-548`).
Accepted.

Cost: one extra `shape_line` per revealed heading, and six cached ones per
layout for the margin.

### D13. The margin shrinks with the pane; a marker that does not fit shows inline

**Problem.** D12's margin is a fixed 74 px on each side of the text. Below a
pane of `column + 2 × 74`, the margins stop shrinking and the text narrows
instead: a 600 px window keeps 92 px of empty space on each side. The user
asked on 2026-11-08 for the margins to keep shrinking, and for a marker that no
longer fits to show inline rather than be clipped.

**Rule.** `TextElement` keeps D12's measured margin as the margin's maximum,
`margin_max`, and adds a floor, `theme::EDITOR_MARGIN_MIN` (16 px). For an
element `E` wide:

- `margin = clamp(floor((E − column) / 2), EDITOR_MARGIN_MIN, margin_max)`, a
  whole pixel: `column` is fractional (`ch × BODY_COLUMN_CHARS`), and D12 keeps
  the text's origin on a whole pixel so a live resize does not drift it;
- the text starts at `margin` and is `max(0, min(column, E − 2 × margin))` wide.

The element is still capped at `column + 2 × margin_max` and centred, so a wide
pane is unchanged. As the pane narrows, both margins shrink together while the
text keeps the full column; once they reach the floor, the text narrows.
`margin_max` stays the widest marker over the six levels, not over the
headings in the note: a margin that jumps when a `######` is typed would move
every line.

**A revealed marker hangs only where it fits.** `LayoutInputs` and
`DocumentLayout` gain `margin: Pixels`, the room left of the text, with
`DocumentLayout::margin()` beside `wrap_width()`. `NameField` passes `px(0.)`:
it parses with `Markdown::default()` and never meets a heading. `line_prefix`
takes `margin`. A revealed marker's shape is already made there; when its width,
extra spaces included, exceeds `margin`, the line gets no `Prefix` and the
marker's `Conceal` goes back to the text's conceals. The
line then takes the path `> # q` already takes (D12, "only at the line start"):
the marker is inline, revealed by the same reveal rule, and may re-wrap the
heading. A concealed marker shows no glyph in either path, so it keeps D12's
`Hanging` with `shaped: None`, and D12's coordinates on a concealed heading are
unchanged. The decision is per line: at one width, `# T` can hang while
`#### T` shows inline. It replaces D12's "a marker with extra spaces paints
into the padding": `#   T` wider than the margin now shows inline.

**D12's invariant narrows.** "No reveal of a heading can move text" holds when
the marker fits the margin. In a pane too narrow for it, a reveal shifts the
heading's text right by the marker and can add a row, as slice 3 did. The
heading moves only while the caret is on it, and only in a narrow pane.
Accepted.

**The cache.** In the band where the margins shrink, the wrap width stays the
column while `margin` changes, so `wrap_width` alone no longer keys the layout.
`is_shaped_from` adds: `margin` equal, or the layout read no margin.
`reads_margin` answers the second: some touched conceal starts a line and is a
heading's marker, so a revealed marker can hang or show inline. The touched
elements already match when the margin is compared, so the check reads the new
inputs. `> # q` starts no line and does not count. Without such a marker no line
depends on `margin`, so a resize in that band reshapes nothing. With one, a
resize reshapes, and `carry_or_shape` carries every line whose display text did not
change.

**Every width source carries the margin.** `TextElement`'s per-frame stash
keys each layout by the `(text width, margin)` it was measured for, never by
the layout's own fields: a reused `last_layout` keeps an older `margin`. In the
band every entry has the column as its text width, so a lookup by wrap width
alone hands a second measure the first one's layout, hanging where it should be
inline. `Stash::take` matches the nearest element width, and prepaint places
the text at that entry's margin; it never recomputes the margin from the
snapped bounds, so measure and paint agree on hang or inline. `reshape` and an
intrinsic query take `margin` from `last_layout`, as they take `wrap_width`, so
a keystroke on a heading in the band still shapes once (`shapes.rs`). A resize across a marker's width toggles that
heading between hanging and inline; no hysteresis, since it happens only with the caret on a heading
during a resize.

Rejected:

- *Clip the marker at the pane's edge.* It shows half a `#`, which reads as a
  rendering bug.
- *A margin measured over the note's own headings.* It changes the column's
  position when a heading of a new level is typed.

## Deferred

| Item | Trigger |
|---|---|
| Task boxes as checkboxes, quotes as a bar, code blocks with a background and hidden fences | The user asks, now that concealment has shipped. Each needs painting beyond text: a quad per row. |
| Conceal a setext underline | Its line would become empty but still take a row. Needs a line of zero height; wait for a user who writes setext. |
| Clickable links, inline images | `inkra-index` resolves `[[target]]`; images need an element beside text. |
| Reveal on the caret only, keep the selection concealed | Drag-selecting across concealed elements shifts text under the mouse enough to hurt, in a render pass or in use. |
| Block-scoped reparse | A profile shows the parse above 1 ms on a real note. The bench puts that near 500 KB. |
| Windowed shaping (lay out visible lines only) | The Measure slice's profile of typing a fence at the top of a large note shows the next frame above 16 ms. This is the roadmap's Virtualization item, with a measured trigger. |
| Table styling | A note with a table reads badly in a render pass. |
| Highlighting inside fenced code | A user asks. |
| Hang a heading marker inside a quote or a list item (D12) | A user writes headings in containers and the re-wrap on reveal hurts. Needs a margin that starts after the container prefix. |
| Space above headings (Zed's `mt_6` on H2 and below) | A render pass shows headings cramped against the paragraph above. The slice-3 `editor-markdown` render already shows H1 and H2 nearly touching. The space fits the per-line `top` as a gap before the heading's rows, and leaves `row_height` alone. |

## Tests

`inkra-markdown`, plain `#[test]` plus `proptest`:

- One test per row of D2's table: exact spans and exact conceals.
- Nesting and containers: `***x***`, a link inside strong, strong inside a
  heading, `> a\n> b`, `> - a\n> - b`, a fence inside a quote.
- `is_active` and `touched`: caret before, inside, after, and at each end of an element; a
  selection across two elements; nested elements.
- `display_line`: `to_display(to_buffer(d)) == d` for every display grapheme
  boundary `d`; both maps are monotonic; `to_buffer` biases left at abutting
  markers (`**a***b*`); `to_display` clamps inside a marker; a bullet maps
  `•` to `-` (D5).
- Block ranges: `# T\nx` with the caret at 4 does not reveal the heading;
  `- x` with the caret at 2 does not reveal the bullet.
- Proptest on arbitrary strings, and on strings biased toward markdown syntax
  characters: spans sorted, non-empty, non-overlapping, on char boundaries;
  every conceal's marker inside its element and inside one line; for any
  selection, no active marker touches it; `parse` never panics.
- `cargo mutants -p inkra-markdown`, zero missed.

`inkra-app`, headless `#[gpui::test]`:

- A line with `**b**`: concealed display text with the caret elsewhere,
  revealed with the caret on it, concealed again after moving away.
- A click on a concealed line lands on the content side (D5).
- Up and down across a concealed line keep the goal x.
- Heading rows: tops, heights, the caret quad height, `row_for_y`.
- `reveal_cursor` after a move that reveals an element scrolls by the new
  layout.
- `line_runs`: run lengths sum to the display line's length; a marked range
  keeps its span's font.
- `Markdown` refresh: present before any keystroke, refreshed by an edit,
  restored by an undo, replaced by `open_buffer`, refreshed by a write outside
  the gates.

D12:

- `inkra-markdown`: `Heading.marker` for `# T`, `###### T`, `#`, `#\tT`,
  `# T ##` (the opening sequence only) and a setext heading (`None`). `# T ##`
  conceals one marker; ` ##` keeps a `Marker` span. Proptest: `marker` is
  `Some` iff the heading is ATX, it starts with `#`, it equals exactly one
  conceal's marker, and every byte of it carries `Marker`.
- Revealing a heading keeps the row count and every row top: the D12 repro
  lays out 2 rows in both states.
- The body starts at `x = 0` in both states. This replaces
  `a_heading_is_shaped_at_its_own_size`, which asserted the old inline width.
- A caret inside the marker is at a negative `x`, as tall as the heading row;
  at `marker.end` it is at `x = 0`.
- A click at a negative `x` on a revealed heading lands in the marker; on a
  concealed one at `line.start`; on a body line at the line's start. A click
  at `x >= 0` on a heading lands at `marker.end` or later.
- `Home`, down, up on a revealed heading whose marker hangs comes back to
  `line.start`. On an inline heading it lands at `marker.end`: back on the
  line, the marker is concealed, so the line is `Hanging` again and
  `offset_on_row` sends `x >= 0` to `marker.end`.
- A selection across a heading covers the marker; the IME rectangle of a
  range in the marker is in the margin.
- `> # q` keeps its inline conceal.
- Margin (D13): a wide window keeps the column centred at its old `x`; a pane
  between `column + 2 × EDITOR_MARGIN_MIN` and `column + 2 × margin_max` keeps
  the full column with two equal margins; a narrower one keeps the floor on each
  side and narrows the text; a pane narrower than both floors gives the text
  zero width.
- A revealed `# T` hangs when its marker fits the margin, with its text at the
  concealed `x`; in a narrower margin it shows inline, its text shifted by the
  marker, and a click or the caret in it maps through the display text. Both
  `# T` and `#### T` at one width where only the second does not fit: one
  hangs, one is inline.
- A resize in the shrinking band reshapes nothing without a revealed marker
  (`SHAPES` unchanged) and reshapes with one.
- Two measures in one frame at two widths in the band, the caret on
  `#### T`: prepaint paints the layout measured for its own width and margin.
- A keystroke on a heading in the band shapes once.
- The text's origin `x` is a whole pixel at every width in the band.
- A shot scene: a narrow pane, the caret on a heading whose marker shows inline.
  `editor-heading-revealed` still shows `# ` dimmed in the margin at its own width. Tests that assume the text starts 16 px in from the pane change
  their constants, not their intent.

No key binding is added. Existing `simulate_keystrokes` tests must pass
unchanged; their notes carry no markdown syntax, or it is revealed under the
caret.

Render, before merge: `shot` scenes registered in
`crates/app/src/shot/scenes.rs` for a concealed paragraph, the caret inside
bold, headings H1 to H3 with a wrapped H1, a list, and every D2 construct.

## Slices

Each slice ships on its own and is looked at in a render.

1. **Styled source.** The crate, `Styles`, D3, every D2 row's spans, D9's
   theme mapping and merge, D10. No concealment yet: markers are dimmed.
2. **Concealment.** In three steps, each green: `DisplayLine` and
   `DisplayOffset` with every `layout/` conversion routed through them and
   nothing hidden (the existing suite is the test); then `Conceal`, D4 and D7
   for headings, emphasis, strong, strikethrough, inline code and links; then
   bullets. This slice makes the one mode.
3. **Heading sizes.** D8: per-line size and height, row tops, every
   `line_height` caller.
4. **Measure.** A criterion bench of `parse`; a manual profile on a 500 KB
   note of an ordinary keystroke, a caret move that reveals, and typing a
   fence at the top. Figures written back here; a fired trigger is recorded
   in Deferred and `docs/roadmap.md`, not built.
5. **Hanging heading markers.** D12, in four green steps: the explicit
   opening conceal and `Heading.marker`, with the closing sequence dimmed and
   no longer hidden; then the margin measured and reserved, with no marker
   painted yet; then `Hanging`: its shape, coordinates and paint; then the
   `editor-headings` and `editor-heading-revealed` renders, with the second
   scene's claim corrected.

## What the review changed

The first draft was a source mode, reviewed adversarially before the user
chose one live mode. From that review:

- D3: a per-container marker rule missed `> ` on continuation lines. It is a
  global rule over leaf events.
- D9: a colour change was said to cost no shaping. It does: colour splits font
  runs, and font runs are the cache key.
- D10: the parse was first refreshed only in two gates, with an assert in
  `request_layout`. Tests write outside the gates, and `EditorView::new`
  would have rendered unstyled. The refresh is lazy, in four places.
- D9: styles are resolved once per distinct value, not per run.
- D2: images, link reference definitions and frontmatter fences were missing.

After slice 1, the user asked for links to conceal too, and for concealment
before heading sizes. D2, the slices and D5's biases changed with it: `End`
had needed two presses past a hidden closing `**`.

A second adversarial review, of this live-preview version:

- D2: block ranges included their trailing `\n`, so the line after a heading
  revealed it. Ranges are clipped before the line ending.
- D2: the bullet element was `- `, which a caret at the content's start
  touches. It is now `-` alone.
- D5: `to_buffer` was ambiguous where two concealed markers abut, and the
  round-trip test could not hold. The map biases left, and the test runs from
  display to buffer. Display indices got a newtype, `DisplayOffset`.
- D8: three `line_height` callers were listed; there are nine, including
  `paint`, which would have mis-spaced a wrapped heading.
- D6: the per-line lookup of spans and conceals is a cursor, not a scan.
- Slices: the display plumbing ships first with an identity map, so the
  concealment slice carries only the reveal rule.

## Corrected in implementation

- **A thematic break inside a quote first kept only `Marker`.** D3 requires the
  enclosing tags as well. It is fixed, and tests pin `> ---` and `- > ---`.
- **Four `replace | with ^ in options` mutants survive.** They are equivalent:
  pulldown-cmark's `Options` flags are disjoint bits, so `^` and `|` give the
  same set. `.cargo/mutants.toml` excludes them and records the proof.
- **The syntax-heavy proptest drew one character at a time.** It almost never
  built a block. A second property now assembles the input from markdown
  fragments.
- **`EditorView::spans` was briefly an associated function.** clippy's
  `unused_self` rejects a method that ignores `self`. It became a method once
  it read the parse.
- **The shot fixture lost six blank lines.** Without them it fits the 1000×640
  shot window. Every block still starts a new construct.
- **`app` gained its `inkra-markdown` dependency with the crate, not in Task 6.**
  So CLAUDE.md never described a dependency that did not exist.
- **A test pins the parse refresh at the top of `reshape`.** Without the
  refresh, an edit is shaped with the previous parse, and every other test
  still passed.
- **pulldown-cmark 0.13.4 re-emitted events inside a wikilink with an empty
  alias.** Whole subtrees came twice. `Unrepeated` skipped any event that
  started at or past the innermost open element's end, depth-balanced. This
  also fixed a flaky slice-1 proptest, which failed in about 2.7% of runs.
  0.13.5 fixes the repeat upstream (RUSTSEC-2026-0414, pulldown-cmark #1102),
  and `Unrepeated` is gone.
- **ATX detection reads the syntax.** It looks for 1–6 `#` then a space, tab,
  newline or end, at the heading's range start. pulldown starts that range at
  the `#`, even when the heading is indented. `"#a\n==="` is setext and
  conceals nothing.
- **A continuation line keeps its container prefix.** On such a line of a
  concealable element, the leading spaces, tabs and `>` stay visible. The rest
  of the element's syntax is concealed.
- **An indented bullet's item range starts at the indentation.** The bullet is
  the first byte after spaces and tabs. Only `-`, `*` or `+` becomes `•`. The
  list-kind stack was dropped: the byte decides.
- **`to_buffer` snaps an index strictly inside a replacement.** It goes to the
  replacement's display edge, then applies the bias. gpui never produces such
  an index: `closest_index_for_x` returns glyph boundaries only.
- **A wikilink with an empty alias, `[[a|]]`, is all markers.** It has no
  alias to show, so by D2 and D3 the whole link conceals until revealed.
  pulldown-cmark 0.13.4 emitted the `]]` as link text, which kept it visible.
- **Performance, release build, one `shape_document` on 5,000 lines with
  20,000 conceals:** 1.2 ms plain text, 4.5 ms concealed, 5.2 ms revealed.
  Concealment adds nothing over revealed. The gap to plain text is slice 1's
  style runs. Debug builds overstate it about 30×. The windowed-shaping
  trigger (16 ms) has not fired.
- **Two UX notes, accepted as in Obsidian.** Select up to the middle of a `**`
  and type: the other delimiter stays. Backspace after a revealed element
  deletes the last character of its delimiter.
- **`inkra-markdown` emits `Markdown::headings`.** The app cannot find a
  heading line from spans alone. A setext underline and an empty `#` carry the
  same styles, but only the `#` line takes the heading's size. Each range
  covers the heading's text lines, without a setext underline.
- **`heading_text` has no ATX branch.** An ATX heading is one line (CommonMark
  §4.2). Cutting at the last `\n` leaves it whole and drops a setext
  underline.
- **Heading sizes take Zed's preview scale over our 18 px body.** H1 is 34 px
  on a 44 px row, H2 27 on 36, H3 22 on 30, H4 20 on 28. `theme` holds them in
  `heading_metrics`. H5 and H6 floor at the body size. Zed makes H6 smaller
  than body text, and a note heading smaller than its paragraph reads as a
  caption.
- **`DocumentLayout::caret_bounds` places the caret.** The editor and
  `NameField` both draw it from there. It is as tall as its row, so a caret on
  a heading is as tall as the heading.
- **The reveal margin stays in body lines.** D8's table moved it to the row's
  height. Zed counts `vertical_scroll_margin` in rows of one `line_height`, so
  a caret on a heading must not widen the margin. `scroll::reveal` takes the
  body line height as its margin unit.
- **The reveal-margin test needs lines after the heading.** At the document's
  end, gpui's scroll limit clamps both the right and the wrong margin to the
  same offset.
- **Headings are the one parse output that spans lines.** A stale `Markdown`
  mis-sizes every line a stale heading range still covers, for that frame.
  D10's "one frame, no panic" still holds: every write reshapes, or the
  observer refreshes the parse, before paint. A future writer outside the
  gates must keep that: refresh the parse before the next layout.
