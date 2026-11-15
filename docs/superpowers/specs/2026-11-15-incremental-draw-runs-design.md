# Incremental layout: rebuild only the lines an edit changes

Status: **proposed** 2026-11-15, amended after red-team review the same day.
Builds on the perf batch (`2026-11-02-perf-batch-design.md`, S2 and S3 carry
shapes), soft wrap (`2026-09-23-soft-wrap-design.md`, D2 and D6), the
decoration layer (`2026-11-13-decoration-layer-design.md`, D7 and D11) and
code highlighting (`2026-11-14-code-highlighting-design.md`). Read their
decisions first.

## Why

An audit proposed building draw runs only for changed lines. It claimed a
gain of about 2 ms and 10–16 MB per keystroke on a 500 KB note. The
measurement below shows a larger and different cost.

- `shape_document` costs 17–22 ms per keystroke at 500 KB, and allocates
  129–238 MB.
- A reshape with identical inputs costs the same. So CoreText is not the
  cost: one `shape_text` call runs per keystroke. The O(lines) loop is.
- Runs are about 30 % of that loop. Display lines, prefixes, rows and the
  collect of 3144-byte `LineLayout`s make up the rest.

So building runs for changed lines only is not enough. The fix is to carry
the whole layout of every unchanged line, and to rebuild only dirty lines.

## Measured baseline

Method. A throwaway `#[ignore]` test ran on a copy of `main` (`git archive`),
in release, on an Apple M5 Pro with rustc 1.98.1. It used the real CoreText
system through `gpui_apple::AppleTextSystem`, since
`gpui_platform::current_platform` panics off the main thread. Settings: wrap
700 px, margin 76 px, the built-in dark theme. The test landed highlight marks
as `open_chunks` would, then typed 41 times. Each keystroke inserted `x` at an
advancing offset, then ran the parse, `catch_up` + `batch` + `publish` as the
listener does, and `shape_document(Some(previous))`. Tables give medians.
A counting global allocator summed the bytes requested by each `alloc` and
`realloc`. That is churn, not live memory. Ranges span three runs.

Notes:

- *1000 blocks*: `# Title`, then 1000 × (a prose section with a heading,
  emphasis, links, a quote, a list, a task, and a Rust block cut from
  `highlight/benches/fixtures/rust-5000.txt`). 528,389 B, 19,399 lines.
- *One block*: about 200 KB of the same prose, then one 300 KB Rust block.
  500,099 B, 15,454 lines.

Per keystroke:

| Case | Parse | Highlight | `shape_document` | Same inputs | Requested |
|---|---|---|---|---|---|
| 1000 blocks, colours, type in prose | 1.92–2.04 ms | 5–7 µs | 20.2–21.9 ms | 19.8–21.7 ms | 238 MB, 153,701 allocs |
| 1000 blocks, no colours | 1.93–2.04 ms | 0–1 µs | 16.4–17.6 ms | 16.5–17.6 ms | 227 MB, 120,478 allocs |
| One block, colours, type in prose | 1.39–1.43 ms | 2 µs | 17.4 ms | 16.8–19.1 ms | 129 MB, 134,081 allocs |
| One block, colours, type in block | 1.39–1.41 ms | 2.19–2.26 ms | 17.0–17.1 ms | 16.6–16.8 ms | 129 MB, 134,064 allocs |

Parts, measured alone on the final state of each note:

| Part | Colours | No colours |
|---|---|---|
| `Drawn::colors_in` over every line | 927–940 µs | 111–118 µs |
| `line_runs` over every line (`colors_in` included) | 5.8–6.4 ms | 2.4–2.6 ms |
| `runs == runs` over every line | 0.67–0.85 ms | 0.28–0.35 ms |
| Collect N items of 3144 B through a map | 2.8–4.9 ms | — |
| Same collect at 192 B | 0.07–0.36 ms | — |
| Collect N placed records of 32 B, with an `Rc` clone each | 14–20 µs | — |
| Parse diff, common prefix and shifted suffix over the five vectors | 44–75 µs | — |
| Opening from scratch, `shape_document(None)` | 105–157 ms | 28–29 ms |

Runs per note: 50,400 → 99,412 with colours (1000 blocks), 36,473 → 102,103
(one block). Sizes: `TextRun` 144 B, `LineLayout` 3144 B, `Option<Prefix>`
3024 B, `ShapedLine` 2984 B, `DisplayLine` 40 B, `Row` 24 B. `ShapedLine`
holds `SmallVec<[DecorationRun; 32]>` inline (`gpui/src/text_system/line.rs:43`).
`Prefix` holds it inline in `Hanging` and `GutterLayout`.

About 8–11 ms of the loop stays unattributed: display lines, prefixes,
`rows`, the shown `String`, labels, blocks and bars. This design skips all of
them for clean lines, so their exact split does not change it.

The audit's claims, checked:

| Claim | Measured | Verdict |
|---|---|---|
| Parse 1.29 ms | 1.39–2.04 ms | Roughly right |
| Highlight 5 µs | 2–7 µs in prose; 2.2 ms typing in the 300 KB block | Right for prose only |
| Draw all lines 0.89 ms | `colors_in` alone is 0.93 ms; the whole loop is 17–22 ms | Wrong scope |
| Total 2.2 ms and 4.1 ms | 19.5–24 ms | Wrong by 5–10× |
| `colors_in` 908–1184 µs | 927–940 µs | Right |
| Runs 37.8k → 103.8k, 144 B each | 36.5k → 102.1k; 144 B | Right |
| 10–16 MB churn | Runs alone: 14–15 MB. Total: 129–238 MB | Runs right; total wrong |

The perf batch measured 11–13 ms end to end at 500 KB on another note, before
the syntax layer. The two are not comparable. The S0 bench reproduces both
notes in the repo.

## Terms

- **Dirty line**: a line whose `LineLayout` may differ from the previous one.
  It is rebuilt.
- **Clean line**: any other line. Its layout is carried and only placed.
- **Placement**: a line's `start`, `top`, `first_row` and `block`. It is the
  only data that changes on a clean line when lines above it change.
- **Full rebuild**: every line is dirty. It is today's path.

## Design

### D1. The unit is the line, and a clean line carries its whole layout

Runs are 30 % of the per-line work, so carrying only runs leaves about 12 ms.
A clean line therefore carries its shape, display line, rows, prefix and
label. Its runs are not rebuilt or compared, and `colors_in` is not called
for it. A dirty line runs today's per-line code unchanged.

### D2. Every line stays shaped; runs exist for dirty lines only

A frame needs every line's row count. The measured height, `top_for_row`,
`row_for_y` and the scroll clamp read all lines (soft-wrap D2). A row count
needs a shape built from styled runs: heading sizes, bold and conceals change
glyph widths. So runs cannot exist for drawn lines only. They are built for
dirty lines, and a carried line keeps the shape its runs produced. The open
risk the audit flagged, soft wrap, is closed this way: a shape is carried only
when its line is clean, and the wrap inputs are part of what makes it dirty
(D5). Shaping visible rows only is the alternative; see "Alternative
considered".

### D3. Lines become line-relative and shared

`DocumentLayout.lines` becomes `Vec<Placed>`:

```rust
struct Placed {
    start: ByteOffset,
    top: Pixels,
    first_row: usize,
    /// The index in `code_blocks` of the block holding this line.
    block: Option<usize>,
    line: Rc<LineLayout>,
}
```

A carried `LineLayout` must hold nothing that a change above it can move.
Every field of today's `LineLayout` (`editor/layout/mod.rs`), audited:

| Field | Today | After |
|---|---|---|
| `shape` | No offset. `LineShape::new` takes `start` only for a warning | Unchanged |
| `range` | Absolute | A length |
| `display` | Absolute `line` and hidden ranges | Relative to the line (open question 3) |
| `first_row`, `top` | Running sums | Move to `Placed` |
| `rows` | `Row.start` and `Row.caret_end` absolute | Relative |
| `row_height` | No offset | Unchanged |
| `prefix` | `Hanging.range` and `GutterLayout.range` absolute; `GutterLayout.geometry` from the line above | Ranges relative; geometry is cross-line state (D7) |
| `block` | An absolute index into `code_blocks` | Moves to `Placed` |
| `label` (new) | The label shape sits in `BlockLayout.label` | The fence line keeps its label shape, boxed (D7) |

`block` cannot stay in `LineLayout`. A fence typed above shifts every later
index, and `block_scroll`, `scroll_bar` and `block_clip` index `code_blocks`
directly, so a stale index paints with another block's scroll or panics. The
placement pass derives `Placed.block` with a cursor over the new
`code_blocks`, as `line_holder` does today.

Readers go through `Placed`, which adds the start back. All readers are in
`editor/layout/` (about 45 sites in six files). `Placed` is 48 B. The 14–20 µs
collect was measured at 32 B.

The previous layout is an `Rc<DocumentLayout>` shared with the element
(`editor/mod.rs`, `layout_at`). `shape_document` sees it by reference and
cannot move lines out. An `Rc<LineLayout>` is carried by a clone, and shifting
it writes only the `Placed`.

### D4. A prefix's shape moves to the heap

`Hanging.shaped` and `GutterLayout.shaped` become `Option<Box<ShapedLine>>`.
`LineLayout` drops from 3144 B to under 256 B. The collect then drops from
2.8–4.9 ms to under 0.4 ms, and the churn by about 100–200 MB per keystroke.
This holds on its own, before any carry, so it ships first (S0). As built,
the gain is larger: see "As built".

### D5. The dirty set comes from four sources, in new coordinates

`shape_document` takes a sorted, merged list of dirty line ranges in the new
text. It is the union of:

1. **Splices.** `buffer.edits_since(previous.at)`, each grown to whole lines.
2. **The parse.** `inkra_markdown::changed(old, new, splices)` (D6).
3. **Decorations.** `Drawn::changed_since(previous revision, buffer)` (D8).
4. **Selection and composition.** The marker line of each conceal whose
   activity flipped (D9). Also the lines of the old and new marked range.

The marked range changes with no splice. `unmark_text` clears it
(`text_input.rs`, `EntityInputHandler::unmark_text`). A restore drops it
through `TextInput::reset_state`. So the marked lines are a source of their
own, and they ship with the first carry (S1a): without them a carried line
keeps a stale IME underline.

Any source that cannot answer makes the whole note dirty:

- the splice log no longer reaches the mark (256 entries);
- the previous layout was shaped from another buffer;
- the parse diff answers `None`;
- the decoration log cannot answer (D8).

The existing `is_shaped_from` clauses still force a full rebuild on a style,
theme, wrap width, margin or `code_block_wrap` change.

**Buffer identity.** An `EditMark` counts from zero in every `Buffer`, and
`Version` does too. A layout from another buffer would find no splices and
carry every line. Production avoids it today only because `TextInput::rebind`
drops `last_layout`. `Buffer` gains a process-unique id from an atomic
counter, read by `Buffer::id`. `Buffer` is not `Clone`, so no two buffers
share one. `DocumentLayout` records the id next to its mark. A mismatch is the
second fallback above.

### D6. The parse diff compares what the layout reads, mapped through the splices

`changed` lives in `inkra-markdown`, which owns `Markdown`. It returns byte
ranges in the new text, or `None` to fall back. It scans each vector by common
prefix, then by common suffix shifted by the net delta. The layout reads this
projection:

| Item | Compared | Lines dirtied when it differs |
|---|---|---|
| `Span` | range, styles | its line |
| `Conceal` | element, marker, replacement | its marker's line |
| `Gutter` | range, quotes, depth | its line, then D7 |
| `Heading` | range, level, marker | all its lines |
| `CodeBlock` | range, language; not `shape` | its fence lines, and the lines whose membership changed |

`changed` comes in two forms.

- **Coarse (S1a).** The items between the common prefix and the common suffix
  are the middle. Prefix items are unshifted and suffix items shifted, so
  every old middle item maps inside the new middle region. That region is
  dirty, grown to whole lines. Typing in prose dirties the spliced line and
  its neighbours. Typing inside a code block dirties the whole block, since
  its `CodeBlock` range changed.
- **Mapped (S2).** It maps each middle item through the splices. A splice
  strictly inside an item grows it, as `map_chunk` does. A mapped item equal
  to its new counterpart is clean. Typing in a 300 KB block then dirties one
  line.

`CodeBlock.shape` changes on every edit inside the block: it fingerprints the
content line ranges. The layout never reads it, so comparing it would dirty all
7,000 lines of the 300 KB block on each keystroke. A new or removed block
dirties all its lines. So typing a fence restyles every line after it, which
is correct.

The mapped form needs to map an offset through splices. Two crates need it, so
it goes in `inkra-text` beside `Splice`. No public helper exists today: only
`Diff::map` and the private `map_mark` in `inkra-editor` (`layer.rs`,
`Layer::shift` → `map_chunk` → `map_mark`).

### D7. Cross-line state runs until it stops feeding forward

The loop has three cross-line dependencies.

- **`GutterCarry`.** A lazy quote line takes its geometry from the last line
  with all its `>` (`2026-09-30-markdown-blocks-design.md`, D3).
  `GutterCarry::layout` measures only a line with `quotes.len() == depth`.
  Every other gutter line keeps the carried geometry, partly lazy lines
  included, and the full pass never resets it on a line with no gutter.
  - **Stop.** After a dirty run, the rebuild continues while the next line has
    a gutter with `quotes.len() < depth`. It stops at a line with no gutter or
    with `quotes.len() == depth`; the latter measures afresh. `Gutter.range`
    is empty only on a fully lazy line, so it is not the test.
  - **Seed.** A dirty run's carry starts from the geometry of the nearest line
    above with a gutter, at any distance, not from `Geometry::default()`. The
    forward walk keeps the last gutter geometry it passes, carried lines
    included. The rule relies on a parser property: a line with no gutter
    closes every quote, and a quote's first line holds all its `>`. So no lazy
    line follows a line with no gutter. A debug assertion and the oracle check
    it.
- **`Blocks` and bars.** A block's background, width and maximum scroll read
  every line of the block. The placement pass rebuilds `BlockLayout`s and bars
  over every line from carried data. It reads widths only and shapes nothing.
  The fence line keeps its label shape in its `LineLayout`, so the pass needs
  no `Labels` for clean lines.
- **Placement.** `top`, `first_row` and `block` are running values over every
  line. The placement pass writes them.

The placement pass stays O(lines). It is the floor of this design: a 48-byte
write per line, plus block and bar arithmetic (unmeasured, see Unverified).

### D8. Every decoration bump logs what moved

Today `Entry::bump` moves the entry's revision, and `is_shaped_from` compares
it. With highlighting on (`default_on`), the revision moves on every edit:
`HeardLayer::catch_up` reports a move whenever a non-empty layer trails the
buffer, and `decorations::sync` bumps. A carry that treats any revision change
as "whole note" therefore gains nothing with colours on. So the bump carries
its kind, and S1b ships the log:

```rust
enum Moved {
    /// `catch_up` mapped every mark through the splices; no colour changed off them.
    Shift,
    /// Marks changed colour over this range, at this edit mark.
    Range(EditMark, Range<ByteOffset>),
    /// Anything may have changed.
    Whole,
}
```

`Entry::bump(revisions, moved)` appends `(revision, moved)` to a bounded log.
No site can bump without logging. The four sites:

| Site | Symbol | Logs |
|---|---|---|
| Sync | `decorations::sync`, when a layer's `catch_up` moved | `Shift`; `Whole` when a layer was lost |
| Publish | `decorations::publish`, when the marks changed | `Whole` until S3, then `Range` |
| Switch | `decorations::enable`, per entry whose layer switched | `Whole` |
| Seed | `land` in `decorations/open.rs`, when the seed landed | `Whole`: the marks cover the note |

`HeardLayer::catch_up` returns `Result<bool, Lost>` instead of `bool`, so
`sync` can tell a lost layer from a shift. `Layer::catch_up` already returns
`Lost` and clears its marks.

`Drawn::changed_since(revision, buffer)` walks the log after `revision`.
`Shift` adds nothing. `Range` maps its range forward through
`edits_since(mark)`. It answers `None` on any `Whole`. It also answers `None`
when the log no longer reaches `revision`, or when the entry never held it:
`declare` clears the entries, and a new entry restarts its log.

`Shift` adds no range because of `map_mark` (`layer.rs`). It splits a mark
around a splice and shifts the rest. So a mark changes colour only on bytes the
splice touched, and those lines are already dirty through D5.1. The D10 oracle
checks this claim.

`decorations::publish` bumps only when the marks changed. Typing in prose
publishes nothing new, so prose typing with colours logs one `Shift` and
rebuilds one line from S1b on. Typing in a block publishes a change, which is
`Whole` until S3.

**S3.** `Layer::publish` returns `Result<Option<Range<ByteOffset>>, Lost>`
instead of `Result<bool, Lost>`. `replace` already compares the marks before
and after in each chunk. It now returns the span from the first to the last
differing mark, not the chunk. Typing in a 300 KB block then recolours a few
lines, not the whole block. `HeardLayer::publish` passes the range through,
and the publish site logs `Range`.

### D9. A selection dirties the marker lines of flipped conceals

Every per-line read of the selection goes through `is_active` on the line's
own conceals: the label (`label_text`), the prefix reveal (`line_prefix`) and
the display line (`display_line`), all in `shape.rs`. A conceal is inactive
when its element touches the selection (`inkra_markdown::is_active`). So a
line depends on the selection only through the conceals whose marker it
holds.

The dirty lines are the marker lines of the conceals whose element is in
exactly one of `touched(old)` and `touched(new)`. `is_shaped_from` already
walks both. Nothing else is dirtied, not even the lines of either selection.

- A conceal's `element` can span lines: the fence conceal's element is the
  whole block (`shape.rs`, `label_text`). A caret entering a block therefore
  rebuilds its fence lines, far from the caret.
- A drag that grows the selection by one line flips only the conceals at its
  moving edge. It rebuilds those lines, not every line it covers.
- A caret move that flips no conceal rebuilds nothing.

### D10. Tests

- **Layout oracle**, a `proptest` in `inkra-app` under `#[gpui::test]`. It
  generates a note from a markdown alphabet: fences, quotes with lazy and
  partly lazy lines, ATX and setext headings, emphasis, links, wikilinks and
  tasks. It drives one `Buffer` through random steps, and lays out after each:
  - single inserts, deletes and replaces at char boundaries, newlines and
    backticks included;
  - batches of k edits before one layout: an `apply_diff` with several hunks,
    and undo or redo of a transaction holding several splices;
  - selection moves, drags and caret jumps across fences;
  - setting and clearing marked text, and a restore's `reset_state`;
  - publishes of a fake syntax layer, a layer switch and a seed landing;
  - occasional wrap width, margin, `code_block_wrap` and theme flips. A flip
    is a full rebuild. The oracle checks that the next step carries from it.

  After each step it compares the incremental layout with
  `shape_document(None)`, line by line (placement, rows, display, prefix,
  label, shape text, runs, wrap), plus `row_count`, `height`, bars and
  `code_blocks`. `NoopTextSystem` gives each char a fixed em width scaled by
  font size (`gpui/src/platform.rs:1659`). So wrapping is deterministic and
  follows heading sizes. Commit the `.proptest-regressions` file.
- **`changed`**, a `proptest` in `inkra-markdown`. For random text and splices,
  every line whose projection differs between the mapped old parse and the new
  parse lies inside a returned range. It covers both forms.
- **`publish` range**, a `proptest` in `inkra-editor`. Every mark that differs
  between before and after lies inside the returned range.
- **The `shape()` helper** (`editor/tests/shapes.rs`, also used by
  `code_blocks.rs` and `margin.rs`) builds a fresh `Buffer` per call. With
  buffer identity (D5), a call with `previous` from another buffer is a full
  rebuild. Its tests stay correct. But `a_carried_line_matches_a_fresh_shape`
  would then never exercise a carry. S1a adds a one-buffer form: it applies
  `after` to the same buffer through `inkra_text::diff` and `apply_diff`, as a
  reload does. The tests that pass `previous` move to it:
  `a_carried_line_matches_a_fresh_shape` (its `===` → `---` case is the setext
  restyle) and `a_composition_reshapes_only_its_line`.
- **Counts.** A test counter `line_build_count` beside `line_shape_count`
  counts dirty lines rebuilt. Named `#[gpui::test]`s in `editor/tests/shapes.rs`:
  - `a_keystroke_rebuilds_only_its_line`, with and without colours
  - `a_newline_above_shifts_without_rebuilding`
  - `a_fence_typed_above_moves_every_block_index`
  - `opening_a_fence_rebuilds_every_line_after_it`
  - `a_setext_underline_restyles_the_line_above`
  - `a_lazy_quote_line_follows_its_gutter`
  - `a_changed_gutter_reaches_its_lazy_lines_and_stops`
  - `unmarking_rebuilds_the_marked_line`
  - `a_layout_from_another_buffer_rebuilds_the_whole_note`
  - `an_overflowed_splice_log_rebuilds_the_whole_note`
  - `landing_a_seed_rebuilds_the_whole_note`
  - `switching_a_layer_rebuilds_the_whole_note`
  - `typing_in_a_large_block_rebuilds_one_line`, with and without colours
  - `a_publish_rebuilds_only_the_lines_it_recolours`
  - `moving_the_caret_into_a_block_rebuilds_only_its_fence_lines`
  - `a_drag_rebuilds_only_its_moving_edge`

  The 13 existing `shapes.rs` tests stay green. Two of them move to the
  one-buffer helper, as above. `a_composition_reshapes_only_its_line` keeps
  its count of 1 only because the marked lines are dirty from S1a (D5.4).

### D11. The bench

`crates/app/benches/layout.rs` uses criterion (a workspace dependency) and
gpui's `bench-support` feature, with `gpui_platform::bench_text_system()`. A
criterion `main` runs on the main thread, which the Mac platform needs. It
builds the two notes above and measures one keystroke's `shape_document` in
three cases: the 1000 blocks typing in prose, and the one block typing in
prose and in the block. Each case runs with and without colours. Run it with
`cargo bench -p inkra-app --features bench --bench layout -- --quick`. The
`bench` feature gates `inkra_app::bench`, which reaches the `pub(crate)`
layout as `shot` does. It is a before/after
comparison, not a gate. Each slice records its numbers in an "As built"
section here.

Expected after S4: `shape_document` per keystroke under 0.5 ms (parse diff,
placement pass, a few dirty lines), and under 1 MB requested. That is a gain of
about 17–21 ms and 130–240 MB per keystroke. The estimate is unverified until
S1a's bench. The parse, 1.4–2 ms, becomes the largest cost of a keystroke.

### D12. Crate ownership

| Crate | Change |
|---|---|
| `inkra-text` | `Buffer::id`; map an offset through splices, with a bias |
| `inkra-markdown` | `changed(old, new, splices)`, coarse then mapped |
| `inkra-editor` | `HeardLayer::catch_up` reports `Lost`; `Layer::publish` and `HeardLayer::publish` return the recoloured range |
| `inkra-app` | `Placed`, line-relative `LineLayout`, the boxed prefix shape, the dirty set, the carry loop, `Moved` and the decoration log, the bench |

No gpui type goes below `app`. Each new function in `text`, `markdown` and
`editor` takes offsets and returns offsets.

## Slices

Each slice is vertical: it ends with a bench-visible gain, the oracle extended
to its source, and its count tests. Each ends green on `./scripts/check.sh` and
records its bench numbers.

- **S0. Bench and the boxed prefix shape.** Add `benches/layout.rs` (D11) and
  record the baseline. Box the prefix shape (D4). Add a `const` assertion that
  `size_of::<LineLayout>()` stays under 256 B. Acceptance: the bench shows the
  collect saving.
- **S1a. Carry through a prose edit, colours off.**
  - D3: `Placed` with `block`, line-relative `LineLayout` and the label field.
  - The placement pass and the `GutterCarry` rule (D7).
  - `Buffer::id` (D5).
  - The dirty set from splices, the coarse parse diff and the marked lines
    (D5.1, D5.2, D5.4 marked part).
  - The oracle with every step except decorations and selection. Any
    decoration revision change or touched-conceal change is still a full
    rebuild.
  - The one-buffer helper and the count tests for edits and marked text.

  Acceptance: prose typing in the 1000-block note without colours under 1 ms
  of `shape_document`; mutants at zero missed on the coarse `changed`.
- **S1b. Carry with colours on.** `Moved`, the bump at the four sites and the
  log (D8 without S3's range), `HeardLayer::catch_up` returning `Lost`, and
  `Drawn::changed_since`. The oracle gains publishes, a switch and a seed. The
  count tests: a keystroke with colours, landing and switching. Acceptance:
  prose typing in the 1000-block note with colours under 1 ms.
- **S2. The mapped parse diff.** `inkra-text`'s splice mapping and the mapped
  `changed` (D6), with their proptests. The count tests for a large block
  without colours, the setext restyle and fence membership. Acceptance: typing
  in the 300 KB block without colours rebuilds one line and costs under 1 ms;
  mutants at zero missed on both crates.
- **S3. Layer ranges.** The publish range across `inkra-editor` and `app` (D8,
  S3 paragraph), the oracle with ranged publishes, and the publish count tests.
  Start after `layer-compact-marks` merges: it rewrites `layer.rs`. Acceptance:
  typing in the 300 KB block with colours under 1 ms of `shape_document`.
- **S4. Selection.** D9, the oracle with selection moves and drags, and the
  caret and drag count tests. The touched-conceal clause of `is_shaped_from`
  stops forcing a full rebuild. S4 also updates the roadmap's Virtualization
  row with the trigger from "Alternative considered". Acceptance: a caret move
  into a block rebuilds only its fence lines; a one-line drag rebuilds only its
  moving edge.

S1a is the largest slice: about 45 reader sites plus the oracle. If it
overruns one session, it splits by stage, never by layer. First comes the
carry with splices only and the coarse diff answering `None` for any middle
item that reaches outside the spliced lines. Then the coarse region and the
marked lines follow.

## Risks

- **Soft wrap.** A carried shape is valid only if its text, runs, font size
  and wrap match. Text and runs are covered by D5. A wrap width,
  `code_block_wrap` or margin change rebuilds everything. A line's wrap also
  reads its block membership and prefix, and D6 and D7 make those lines dirty.
- **A missed source.** A layout input that D5 does not cover leaves a stale
  line. The oracle is the guard. Any new `LayoutInputs` field must join D5 or
  the full-rebuild clauses. Any new `Entry::bump` call needs a `Moved`, so it
  cannot skip the log.
- **An absolute value left in `LineLayout`.** It would be stale on every
  carried line below a change. D3's audit lists every field. A new field must
  be line-relative or go into `Placed`.
- **Old-to-new line mapping.** A clean new line at `s` maps to the old line at
  `s` minus the net delta before it. A debug assertion checks that both lines
  have the same length.
- **Typing a fence.** It dirties every line after it, so it costs what a full
  shape costs today: 28–157 ms at 500 KB from scratch. This is not a
  regression. See the open questions and "Alternative considered".
- **`layer-compact-marks`.** That branch changes `layer.rs`. S3 rebases on it.
  This spec cites `layer.rs` by symbol for that reason.

## Alternative considered: shape visible rows only

CodeMirror 6 lays out only the viewport. Off-screen lines keep estimated
heights in a height map, which measurement corrects as they scroll into view.
Zed is close: it wraps unstyled text in the background
(`wrap_map.rs`, `WrapMap::flush_edits`) and shapes visible rows only
(`element.rs`, `layout_lines`). Xi's rope science notes discuss the limits of
incremental wrap (part 6; not read for this spec, see Unverified).

What this design leaves as a full rebuild, at 28–157 ms on a 500 KB note:

- opening a note;
- a theme switch;
- a style change;
- a resize below the column cap, where the wrap width changes;
- a margin change that a heading reads;
- a `code_block_wrap` toggle;
- typing a fence, or any structure change that dirties most lines;
- a layer switch, a seed landing, or an overflowed splice log.

Above the column cap a resize changes no wrap width, so it stays cheap.

The viewport alternative bounds every one of these by the screen. It was not
chosen now for three reasons:

- Our row heights depend on styled runs: heading sizes, bold widths and
  conceals. An estimate is wrong for most unmeasured lines, so the scroll bar,
  `row_for_y`, reveals and link landing (`Reveal::Top`) would jump as lines
  get measured.
- It needs the element to own its vertical scroll. That is the roadmap's
  `TextElement` scroll-anchor row, not built yet.
- It does not remove this design's need: a viewport layout still needs to know
  which visible lines an edit changed. The dirty set carries over unchanged.

It would win when full rebuilds, not keystrokes, are what a user feels. The
roadmap's Virtualization row (2–3 d) is this alternative. Its trigger today
is "a profile on a real note showing per-frame shaping costs anything". S4
replaces it with this: "after incremental layout ships, a profile on a real
note shows a full rebuild above 16 ms on a common gesture (open, theme switch,
resize below the column cap, typing a fence)". The measured open already
exceeds that at 500 KB, so the trigger turns on real notes and real gestures,
not on the synthetic one.

## Open questions

1. *Keep `LineShape.runs`?* They are compared only for dirty lines now, but they
   stay resident: 14–15 MB at 100k runs. **Recommend** keeping them. The churn
   goes away with S1a, and residency belongs to the memory lead of the
   2026-11-08 bench refresh.
2. *Typing a fence in a large note.* **Recommend** accepting today's cost. If a
   profile shows it, the answer is the viewport alternative above or Zed's
   background rewrap with an interpolated snapshot (`WrapMap::flush_edits`).
   S4's roadmap update covers it.
3. *Make `DisplayLine` line-relative in `inkra-markdown`, or convert in `app`?*
   **Recommend** building it over `0..len` in `app`, since `app` builds one per
   line anyway. Check the other `DisplayLine` users in S1a first.
4. *The parse becomes the floor at 1.4–2 ms.* **Recommend** leaving it out of
   scope. Add a roadmap row, "incremental parse", triggered by the S4 bench.
5. *The oracle's text system.* **Recommend** `NoopTextSystem` for the proptest,
   plus one `#[ignore]` test on CoreText for bold widths.
6. *Where buffer identity lives.* An id on `Buffer`, recorded by
   `DocumentLayout`, or an id inside `EditMark`, so that `edits_since` answers
   `None` for a foreign mark. **Recommend** the id on `Buffer`. `EditMark` is
   `Default` and layers compare it with `edit_mark()`, so changing it reaches
   every layer path for one caller's need.

## Deviations from Zed

1. Every line stays shaped. Zed shapes visible rows only (`element.rs`,
   `layout_lines`) and wraps unstyled text with `LineWrapper`
   (`wrap_map.rs`, `WrapMap::update`). Our wrap depends on styled runs. See
   "Alternative considered".
2. A `Vec<Placed>` is rebuilt per edit in tens of µs. Zed uses a `SumTree` with
   relative summaries, which costs O(log n). At note scale, the vector is
   simpler.
3. One dirty set, from four sources, is built at layout time. Zed composes
   `Patch<WrapRow>`s through a chain of maps (`wrap_map.rs`). We have one
   layout stage.
4. The rebuild is synchronous, with no background rewrap and no interpolation.
   A keystroke dirties a few lines.
5. The parse stays a whole-document pass. Zed reparses incrementally with
   tree-sitter. That is out of scope here (open question 4).

## As built

S0, release, M5 Pro, colours on. The bench times `shape_document(Some(prev))`
alone; the edit, parse and highlight run untimed. `LineLayout` went from
3144 B to 176 B. The allocation count did not change, only the bytes.

| Case | Before | After | Requested before | Requested after |
|---|---|---|---|---|
| 1000 blocks, type in prose | 21.25 ms | 11.87 ms | 238.3 MB | 43.8 MB |
| One block, type in prose | 17.60 ms | 10.18 ms | 128.7 MB | 31.5 MB |
| One block, type in the block | 18.02 ms | 10.09 ms | 128.7 MB | 31.5 MB |

Without colours (`plain`), after the box only. The case was added after the
review, so it has no pre-box figure. Neither lexing nor publishing runs, and
`Drawn` holds an empty layer.

| Case | After | Requested after |
|---|---|---|
| 1000 blocks, type in prose | 7.83 ms | 32.6 MB |
| One block, type in prose | 4.95 ms | 16.5 MB |
| One block, type in the block | 4.91 ms | 16.5 MB |

gpui's `bench-support` turns on criterion's default features (plotters,
rayon), against `docs/tooling.md`; only under the `bench` feature. The
tooling doc records the exception.

S1a, release, M5 Pro, the `plain` cases. Before is draw-s0, after is S1a, both
measured in one sitting.

| Case | Before | After | Requested before | Requested after |
|---|---|---|---|---|
| 1000 blocks, type in prose | 8.95 ms | 0.96 ms | 32.6 MB | 7.5 MB |
| One block, type in prose | 5.83 ms | 0.53 ms | 16.5 MB | 1.0 MB |
| One block, type in the block | 5.65 ms | 2.60 ms | 16.5 MB | 5.6 MB |

Typing in the block rebuilds the whole block: the coarse diff dirties every
line whose `CodeBlock` range changed, until S2. The colours-on cases still
rebuild the whole note on each keystroke, since the decorations revision
moves (S1b): 12.98 ms, 10.69 ms and 10.17 ms after, against 12.82 ms,
11.58 ms and 11.24 ms before.

After review, each block's label is an `Rc<ShapedLine>` shared by its line and
its `BlockLayout`, and `Blocks` sizes its list from the parse. Before, every
layout cloned each label inline, about 3 KB a block, into a list grown from
empty. Before is the table's S1a, after is with the shared label, both measured
in one sitting:

| Case | Before | After | Requested before | Requested after |
|---|---|---|---|---|
| 1000 blocks, type in prose, plain | 0.97 ms | 0.59 ms | 7.46 MB | 1.32 MB |
| One block, type in prose, plain | 0.48 ms | 0.44 ms | 1.03 MB | 1.03 MB |
| One block, type in the block, plain | 2.47 ms | 2.49 ms | 5.59 MB | 5.59 MB |
| 1000 blocks, type in prose, colours | 11.23 ms | 11.28 ms | 39.70 MB | 30.57 MB |
| One block, type in prose, colours | 9.65 ms | 9.99 ms | 29.30 MB | 29.29 MB |
| One block, type in the block, colours | 9.68 ms | 9.63 ms | 29.28 MB | 29.28 MB |

S1a deviations from this spec:

1. `changed(old, new, edit)` takes one `Splice`, the hull of every splice since
   the previous layout, and never answers `None`. The coarse form reads only
   the edit's start, the end of the removed bytes and the shift after them,
   and the hull gives all three.
2. `Splice::map(offset, bias)` ships in S1a, not S2. The old marked range must
   map into the new text (D5.4), and the hull is a `Splice`.
3. The splices compose into the hull through `inkra_highlight::dirty`, which
   already composes them. The dirty lines are the hull's lines; growing each
   splice separately would give the same lines or fewer, for more code.
4. A margin change rebuilds the whole note, even where `is_shaped_from` keeps
   the layout because no line reads the margin. Carrying across it would need
   the per-line reads that `reads_margin` scans for, for a resize-only gain.
5. The line types moved to `layout/line.rs` and the prefix build to
   `layout/prefix.rs`. `shape.rs` and `mod.rs` would otherwise pass their line
   ceilings.
6. The bench's `Typing` moves `Drawn.revision` when the colours move, as
   `decorations::sync` does. With the revision fixed at 0, a carry would keep
   stale colours and the colours-on cases would time the wrong work.
7. A selection change rebuilds the whole note only when a conceal on a clean
   line flips activity, where S1a said any touched-conceal change does.
   Comparing the touched sets through the hull is unsound: an old selection
   inside the removed bytes maps onto the hull's edge, and two elements
   collapse into one. A flip on a dirty line costs nothing, since the line is
   rebuilt.
8. A line with no gutter passes no geometry on, which replaces D7's seed. The
   oracle found a lazy line after a line with no gutter: `gutter()` counts only
   spaces before an inner `>`, so `> \t> ` reads as depth 2 with one `>`. The
   seed's parser property is false, its debug assertion fired, and a lazy line
   could read a gutter far above whose rebuild it did not follow. With the
   reset, a lazy line reads only the line above, so D7's stop rule is enough.
   A fresh layout differs from draw-s0's only on such a line below an earlier
   quote, where draw-s0 drew that quote's bars.
9. The count tests live in `editor/tests/carry.rs`, since `shapes.rs` would
   pass 400 lines. The colours variant of the keystroke test is S1b's.

S1b, release, M5 Pro. Before is draw-s1a, after is S1b, both measured in one
sitting. Bytes are gpui's bench report, "allocated bytes per iteration".

| Case | Before | After | Allocated before | Allocated after |
|---|---|---|---|---|
| 1000 blocks, type in prose, colours | 11.29 ms | 0.60 ms | 30.57 MB | 1.32 MB |
| One block, type in prose, colours | 9.84 ms | 0.44 ms | 29.29 MB | 1.03 MB |
| One block, type in the block, colours | 10.09 ms | 7.28 ms | 29.28 MB | 20.58 MB |
| 1000 blocks, type in prose, plain | 0.62 ms | 0.63 ms | 1.32 MB | 1.32 MB |
| One block, type in prose, plain | 0.45 ms | 0.46 ms | 1.03 MB | 1.03 MB |
| One block, type in the block, plain | 2.47 ms | 2.48 ms | 5.59 MB | 5.59 MB |

Typing in the block still rebuilds the whole block, as without colours: the
coarse parse diff dirties it (S2), and so does the publish's range (S3). Its
lines cost more with colours: `colors_in` and about twice the runs.

The decoration log keeps the last 64 bumps. The oracle drives the real
`decorations` module: publishes at any edit mark seen, with cleared ranges and
chunks; seeds; switching the layer on and off; losing the splice log; and a
splice laid out before its `sync`. With the decoration ranges dropped from the
dirty set, the oracle fails within a few cases from an empty regressions file,
three runs out of three. It also checks D8's `Shift` claim on every step.

S1b deviations from this spec:

1. `Moved` and the log, `RevisionLog`, live in `inkra-editor`, not `app` (D12).
   They take offsets and an `EditMark` and return offsets, and need no
   `Window`, so they get `#[test]` and a proptest there. `app`'s `Entry` holds
   one, and `Drawn` reads it.
2. A publish logs `Range` from S1b on, not `Whole` until S3. The range spans
   the publish's cleared ranges and chunks, at its edit mark. `replace` changes
   marks only inside those ranges once mapped, so the span holds every
   recoloured byte. S3 narrows it to the marks that differ. Without it, a
   recolour of one block rebuilds the whole note, and typing in a block with
   colours could not carry.
3. `HeardLayer::publish` returns `Result<bool, Lost>`, like `catch_up`. The
   publish site logs `Whole` for a lost layer and the range for a recolour; a
   `bool` merged the two. S3 still turns the `bool` into the range.
4. `Drawn::revision` is `Option<u64>`: `None` while a layer with marks trails
   the buffer's edit mark. `colors_in` draws no such layer, and the `sync` that
   brings it back logs only a `Shift`. So a layout drawn without its colours
   would carry uncoloured lines after the `sync`, and a layout drawn with them
   would carry coloured lines into a frame that draws none. A `None` on either
   side rebuilds the whole note. `EditorView::refresh` syncs before every
   layout, so production never pays it. The oracle fails without it.
5. `sync` logs `Whole` for a lost layer even when it held no marks. A layer
   catches up on every `sync` while it is on, so only a layer switched on after
   more than 256 splices is lost there, once.

## Unverified

- The expected gain after S4 (D11), and the cost of the placement pass's block
  and bar arithmetic. S0 and S1a measure them. The placed collect was measured
  at 32 B, not 48 B.
- The number of `DisplayLine` users outside `editor/layout/` (open question 3).
- The split of the 8–11 ms not attributed above.
- That `catch_up` never changes colour off a spliced line (D8). This comes
  from reading `map_mark`; the oracle checks it.
- The content of Xi's rope science part 6, cited from the review, not read.
