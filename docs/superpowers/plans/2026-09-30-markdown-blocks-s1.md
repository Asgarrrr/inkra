# Markdown blocks S1 — the gutter and quotes

> **For agentic workers:** steps use checkbox (`- [ ]`) syntax. Written by an
> Opus planner on 2026-09-30 against `a844001` (branch `docs/markdown-blocks`).
> pulldown-cmark 0.13.4 event ranges quoted below were printed from that tree
> with a throwaway test.
>
> **Red/blue review, 2026-10-01 (binding).** Task 4 and Task 5 now show
> `GutterLayout::new` and `Prefix::bars` in full. The gap rule's prefix skip
> takes a superset of a gutter, so its markers never start inside one; Task 4
> pins that with `a_strong_across_quote_lines_keeps_its_markers_out_of_the_gutter`.
> Sharing one prefix function between the two was rejected: the gap rule also
> skips list indents, which a gutter must not take.

**Goal:** a quote's `>` markers leave the text and move into a gutter, a
hanging indent measured at the body size. Every row of a quoted line, wrapped
rows and lazy lines included, starts at the gutter's right edge. A bar per
quote level runs down the gutter. A caret on a line's `>`s paints them in the
gutter, so no reveal moves text.

**Spec:** `docs/superpowers/specs/2026-09-30-markdown-blocks-design.md`, D1
(`Gutter`), D2 (the `>` row), D3, D7 (`quote_bar`, `QUOTE_BAR_WIDTH`), Tests,
slice S1. Live-preview spec D2–D5 and D12. Rules: `CLAUDE.md`,
`.claude/rules/{editor,app-tests,gpui}.md`.

Every task: failing test first, run it, implement, run it, then `cargo fmt -p
inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown -p
inkra-index -p inkra-tabs` (never `--all`), then commit.

## Spec corrections

Each item deviates from the spec's text or fills a gap in it. Task 7 records
them in the spec's "As built (S1)".

1. **Only a leading quote gets a gutter.** A leading quote is one whose
   ancestors are all quotes. The spec says a gutter runs "from the line's start
   through each `>` of the quotes that contain the line". In `- > a` that range
   swallows the bullet: the `-` would vanish, and its `•` conceal would overlap
   the gutter, which `DisplayLine::new` asserts against (`display.rs:33-41`). A
   quote inside a list item keeps today's inline dimmed `> `. `depth` counts
   leading quotes only. `> - > a` gets a gutter over the first `> ` only.
   Deferred row: "A quote inside a list item gets a gutter | A quote in a list
   is reported."
2. **The `>` conceals of one line share one element.** The element runs from
   the gutter's start through the line's last `>`, not the `>` alone (spec D2
   table). With one element per `>`, `> > x` with the caret at offset 1 would
   reveal one `>` of two, and the gutter would need a shape per `>`. With a
   shared element a gutter has one state: it paints its one shaped text or
   nothing. A caret after the last `>`'s space still reveals nothing, which is
   the point of the spec's narrow element.
3. **A click in a revealed gutter lands on the offset under it.** The spec
   says a click in the gutter lands on the text's start. That holds while the
   `>`s are concealed. While revealed, the gutter behaves as D12's margin
   does: the painted `>` is clickable. One rule: `x` left of the text maps
   through the gutter's shape when it is painted, to the text's start when
   not.
4. **A lazy line is any gutter with fewer `>` than its `depth`.** The spec
   describes only the fully lazy line (empty range, empty `quotes`).
   `> > a\n> b` makes `b` lazy for the inner quote with one `>` present
   (pulldown: inner quote `2..9` covers it). Its range is `> `, its `quotes` is
   `[6]`, its `depth` is 2. One rule covers both: a gutter holding all its
   `>` measures its geometry; any other carries the last measured one.
5. **Where a `>` may stand.** "One `>` for each open quote" needs the
   CommonMark container rule. Each `>` may follow up to three spaces; a
   fourth makes the rest text. `> a\n    > b` is one paragraph whose second
   line is the text `> b` (pulldown: `Text` at `8..11`). "The one space
   after each" also takes a tab: `>\tx` starts its paragraph at 2.
6. **The scene is `markdown-blocks`, kebab-case**, as every catalogue name
   (`scenes.rs:85-290`). A second scene, `markdown-blocks-revealed`, puts the
   caret on a quote's `>`: a revealed gutter is a state worth looking at.
7. **`Gutter` gets no `task` field in S1.** Nothing in S1 reads it. S3 adds it
   with its only reader and its tests. The S1 tests compare gutters through a
   tuple helper, so S3's field breaks none of them.
8. **Files split before they pass ~400 lines.** `editor/layout/mod.rs` is 357
   lines: `Hanging` and the new types go to `layout/prefix.rs`, `paint` to
   `layout/paint.rs` (Task 1). `parse.rs` (350) keeps the event loop; the line
   walk goes to `crates/markdown/src/gutters.rs`. `parse/tests.rs` is already
   663 lines: new tests go to `parse/tests/gutters.rs`, beside `headings.rs`.
   `shot/scenes.rs` is 455: the scene goes to `shot/scenes/markdown_blocks.rs`.
9. **Zed's `next_indent` is a partial precedent.** Zed indents the wrapped
   rows of a line by its own leading whitespace, inside the wrapped text, and
   leaves the first row at 0 (`wrap_map.rs:655-658`). The gutter moves every
   row, the first included, by a width that is not in the shaped text. The
   Deviations section gains this row.
10. **D5's paint order is S2's.** In S1 the bars paint with the text, after
    the selection. A bar covers a 2 px strip of a selection in the gutter.

## Decisions settled from the code

1. **Leading quotes come from the event loop.** `Open` gains `leading: bool`.
   At `Start(BlockQuote)`, the quote is leading when the innermost open
   element (`current`, the parent) is `None` or itself leading. Its range,
   without its trailing `\n`, goes to `quotes: Vec<Range<usize>>`, in start
   order. Every other element gets `leading: false`, so a list item breaks the
   chain. Checked on 0.13.4: `> a\n\n> b` gives quote `0..4`, whose `\n` must
   go, or the blank line would be quoted.
2. **`gutters.rs` walks only quoted lines.** For each line from the first
   quote's line start to the last open quote's end: quotes starting on the
   line join an `open` stack; the line's gutter takes one `>` per open quote;
   quotes ending on the line leave. Cost: the quoted bytes, once. No scan of
   the whole note is added (CLAUDE.md, performance).
3. **The `>` conceals merge by a sort.** `out.conceals` and the quote conceals
   are each sorted by `marker.start`. `sort_by_key` on their concatenation
   merges two runs in linear time: the std docs name "two or more sorted
   sequences concatenated" as the fast case. The proptest pins the order.
4. **No other conceal lies in a gutter.** A gap conceal on a continuation line
   skips its `[ \t>]*` prefix (`parse.rs:250-270`), and a gutter holds only
   spaces, `>` and one tab. So a marker starting before a gutter's end is one
   of its `>`. `shape.rs` therefore drops every conceal starting before the
   prefix's end, one rule for margins and gutters. A proptest holds the
   invariant.
5. **`Prefix` replaces `hanging`.** `LineLayout.prefix: Option<Prefix>` with
   `Margin(Hanging)` (unchanged) and `Gutter(GutterLayout)`. A line gets a
   gutter when `markdown.gutters` has one starting at its start, else a margin
   when a heading's opening marker starts it (D12). Both cannot hold: a
   gutter line starts with a space, a `>`, or lazy paragraph text.
6. **`indent` is the one new term in every conversion.** `LineLayout::indent`
   is 0 for a margin and the gutter's width for a gutter. `x_on_row` adds it
   on every row; `offset_on_row` subtracts it on every row and splits row 0 at
   it (a margin splits at 0, as today). `x_for_offset`, `caret_bounds`,
   `range_bounds` (IME), `selection_quads`, `offset_for_position` and the
   vertical moves all go through these two functions, so none changes.
7. **The gutter's shape is always there.** A margin shapes its marker only
   while revealed (D12). A gutter shapes its source text at the body size on
   every shape: its width and its `>` positions do not depend on the reveal
   state (spec D3). A caret inside the gutter reads its `x` from that shape in
   both states. The shape goes through `show` (`shape.rs:157-172`), so its
   dim and IME underline match the body's. gpui's line cache keys on text,
   runs and size, so a frame with nothing changed reshapes nothing new.
8. **The wrap width is `column − indent`, never below 0.** `shape_text` gets
   `wrap_width.map(|width| (width - indent).max(px(0.)))`. `None` still never
   wraps (`NameField`).
9. **Bars run from a line's top to the next line's top.** `quote_bars` pairs
   each line with the next one's `top` (the last with `height`), so bars of
   consecutive lines touch by construction and cover every row.
10. **A heading in a quote keeps its inline conceal**, as D12 accepts. Its
    gutter is shaped at the body size, so its bar and text start align with
    the body lines around it.

## Invariants

- **G1:** gutters are sorted by `range.start`, one per line; each range starts
  its line and ends at or before the line's end.
- **G2:** `1 <= depth` and `quotes.len() <= depth`; `quotes` is empty exactly
  when the range is.
- **G3:** each entry of `quotes` is a `>` byte inside the range, inside a
  span styled `QUOTE | MARKER`.
- **G4:** the conceals whose marker starts inside a gutter's range are exactly
  its `quotes`, one byte each.
- **G5 (app):** revealing or concealing a line's `>`s changes no row top and
  no `x` of an offset at or after the gutter's end.
- **G6 (app):** every row of a quoted line starts at its gutter's width; a
  lazy line's width and bars are those of the last gutter that held all its
  `>`.
- **G7 (app):** a click left of a concealed gutter's edge lands on the text's
  start of that row.

## Layout

| File | Change |
|---|---|
| `crates/markdown/src/gutters.rs` | new: `Gutter`, `gutters`, `gutter`, `quote_conceals` (~90 lines) |
| `crates/markdown/src/lib.rs` | `mod gutters;`, `pub use gutters::Gutter;` |
| `crates/markdown/src/parse.rs` | `Markdown.gutters`; `Open.leading`; `quotes`; conceal merge (~380 lines) |
| `crates/markdown/src/parse/tests.rs` | five conceal expectations gain the `>`; `syntax_fragment` gains `>` and four spaces; `mod gutters;` |
| `crates/markdown/src/parse/tests/gutters.rs` | new: examples, conceals, proptests G1–G4 |
| `crates/app/src/editor/layout/prefix.rs` | new: `Prefix`, `Hanging` (moved), `GutterLayout`, `Geometry` |
| `crates/app/src/editor/layout/paint.rs` | new: `paint` (moved), `quote_bars` |
| `crates/app/src/editor/layout/mod.rs` | `prefix` field, `indent`, `prefix_on`, conversions; test accessor `prefix_painted` |
| `crates/app/src/editor/layout/shape.rs` | gutter cursor, prefix choice, wrap width, carried geometry |
| `crates/app/src/theme.rs` | `quote_bar`, `QUOTE_BAR_WIDTH` |
| `crates/app/src/editor/tests/mod.rs` | `assert_x` moves here from `hanging.rs`; `mod gutter;` |
| `crates/app/src/editor/tests/gutter.rs` | new: geometry, clicks, lazy lines, reveal, bars |
| `crates/app/src/editor/tests/hanging.rs` | the heading-in-a-quote test reads the gutter |
| `crates/app/src/shot/scenes.rs` | `mod markdown_blocks;`, two catalogue entries |
| `crates/app/src/shot/scenes/markdown_blocks.rs` | new: `QUOTES`, `concealed`, `revealed` |
| Docs | the spec's As built (S1), `CLAUDE.md` Docs list, `.claude/rules/editor.md` |

## Task 0: baselines

- [ ] `./scripts/test-names.sh > target/names-mb-s1-0.txt`.
- [ ] Parse bench: `cargo bench -p inkra-markdown -- --quick 2>&1 | tee
  target/bench-mb-s1-0.txt`. Expected: three `markdown_parse/…` lines with a
  `time:` each. Its section holds one two-line quote, so the gutter walk shows.
- [ ] Keystroke timing on a 50 KB note of quotes, through the off-screen
  harness (the method of `docs/roadmap.md`, Measurements, 2026-09-28). Add a
  throwaway scene to `crates/app/src/shot/scenes.rs`. Put the catalogue entry
  first, right after `const CATALOGUE: &[Scene] = &[`, and the function right
  after `fn empty_no_vault`. Later tasks touch neither place, so the patch
  re-applies cleanly.

  ```rust
      Scene {
          name: "perf-quotes",
          shows: "throwaway: 20 keystrokes timed on a 50 KB note of quotes",
          open: perf_quotes,
      },
  ```

  ```rust
  fn perf_quotes(stage: &mut Stage) -> WindowHandle<AppShell> {
      let section = "> A quote line long enough to wrap once at seventy columns, with **strong** text in it.\n\
                     > > A nested quote line.\n\
                     > > A nested line that runs on\n\
                     with a lazy line.\n\
                     > # A heading in a quote\n\
                     \n";
      let note = section.repeat(50_000 / section.len() + 1);
      let window = stage.open_note(&note);
      stage.focus(window);
      stage.keys(window, "cmd-down");
      let mut times: Vec<_> = (0..20)
          .map(|_| {
              stage.capture(window);
              let start = std::time::Instant::now();
              stage.keys(window, "a");
              start.elapsed()
          })
          .collect();
      times.sort();
      eprintln!(
          "perf-quotes: {} bytes, median {:?}, max {:?}",
          note.len(),
          times[10],
          times[19]
      );
      window
  }
  ```

  Each keystroke types at the note's end, so the edit reparses and reshapes
  the whole note, and `capture` draws a frame before it. Run it three times:

  ```
  cargo run --release -p inkra-app --features shot --example shot -- perf-quotes 2>&1 | grep perf-quotes
  ```

  Expected: `perf-quotes: 50… bytes, median …ms, max …ms`. Record the three
  medians in `target/perf-mb-s1-0.txt`. Then save and remove the scene:
  `git diff crates/app/src/shot/scenes.rs > target/perf-quotes.patch` and
  `git apply -R target/perf-quotes.patch`. `git status --short` prints
  nothing.

## Task 1: split `editor/layout/mod.rs`, no behaviour change

`mod.rs` is 357 lines and S1 adds to it. Move first, under green.

- [ ] Create `crates/app/src/editor/layout/prefix.rs`. Move `struct Hanging`
  and `impl Hanging` (`mod.rs:38-58`) into it, with the fields and both
  methods `pub(super)`:

  ```rust
  //! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12).

  use std::ops::Range;

  use gpui::{Pixels, ShapedLine};
  use inkra_text::ByteOffset;

  pub(super) struct Hanging {
      pub(super) range: Range<ByteOffset>,
      /// `Some` only while revealed; painted left of the text and right-aligned to it.
      pub(super) shaped: Option<ShapedLine>,
  }
  // `impl Hanging { x_for_offset, offset_for_x }` moved verbatim, each `pub(super)`.
  ```

- [ ] Create `crates/app/src/editor/layout/paint.rs` with
  `impl DocumentLayout { pub(crate) fn paint(..) }` moved verbatim from
  `mod.rs:323-356`, with its doc comment and the imports it needs (`App`,
  `Pixels`, `Point`, `TextAlign`, `Window`, `point`, `px`).
- [ ] `mod.rs`: `mod paint; mod prefix;`, `use prefix::Hanging;`; drop the
  imports only `paint` used. `shape.rs` imports `super::prefix::Hanging`.
- [ ] Run `cargo test -p inkra-app editor::`. Expected: `test result: ok.`
  with the same count as before the move.
- [ ] `./scripts/test-names.sh | diff target/names-mb-s1-0.txt -`. Expected:
  no output.
- [ ] Commit: `Split the editor layout: prefix and paint get their own files`.

## Task 2: a quoted line's text starts at the gutter's edge

`inkra-markdown` names each quoted line's gutter; `app` keeps it out of the
shaped text and moves every row right by its width. No `>` shows yet; Task 4
reveals them.

- [ ] Failing markdown tests, new `crates/markdown/src/parse/tests/gutters.rs`
  (`mod gutters;` after `mod headings;` in `parse/tests.rs`):

  ```rust
  //! `Markdown::gutters` (markdown blocks spec, D1): one per quoted line, found from the parse.

  use proptest::prelude::*;

  use super::super::parse;
  use super::syntax_fragment;
  use crate::Styles;

  /// Each gutter as its text, its `>` offsets and its depth.
  fn gutters(text: &str) -> Vec<(&str, Vec<usize>, usize)> {
      parse(text)
          .gutters
          .iter()
          .map(|gutter| {
              let range = gutter.range.start.0..gutter.range.end.0;
              let quotes = gutter.quotes.iter().map(|quote| quote.0).collect();
              (&text[range], quotes, gutter.depth)
          })
          .collect()
  }

  #[test]
  fn every_quoted_line_has_a_gutter() {
      assert_eq!(
          gutters("> a\n> b"),
          vec![("> ", vec![0], 1), ("> ", vec![4], 1)]
      );
      assert_eq!(gutters("a\n> b"), vec![("> ", vec![2], 1)]);
      assert_eq!(gutters("plain\n- a"), vec![]);
  }

  #[test]
  fn nested_quotes_take_one_marker_per_level() {
      assert_eq!(gutters("> > x"), vec![("> > ", vec![0, 2], 2)]);
      assert_eq!(
          gutters(">>x\n> > y"),
          vec![(">>", vec![0, 1], 2), ("> > ", vec![4, 6], 2)]
      );
      assert_eq!(
          gutters("> a\n> > b"),
          vec![("> ", vec![0], 1), ("> > ", vec![4, 6], 2)]
      );
  }

  #[test]
  fn a_lazy_line_has_an_empty_gutter_at_its_depth() {
      assert_eq!(gutters("> a\nb"), vec![("> ", vec![0], 1), ("", vec![], 1)]);
      assert_eq!(
          gutters("> a\n> > b\nc"),
          vec![("> ", vec![0], 1), ("> > ", vec![4, 6], 2), ("", vec![], 2)]
      );
  }

  #[test]
  fn a_line_short_of_its_depth_keeps_the_markers_it_has() {
      assert_eq!(
          gutters("> > a\n> b"),
          vec![("> > ", vec![0, 2], 2), ("> ", vec![6], 2)]
      );
  }

  #[test]
  fn up_to_three_spaces_lead_a_marker_and_a_fourth_makes_it_text() {
      assert_eq!(
          gutters("  > a\n  > b"),
          vec![("  > ", vec![2], 1), ("  > ", vec![8], 1)]
      );
      assert_eq!(gutters("> a\n    > b"), vec![("> ", vec![0], 1), ("", vec![], 1)]);
  }

  #[test]
  fn one_space_or_tab_after_a_marker_joins_the_gutter() {
      assert_eq!(gutters(">\tx"), vec![(">\t", vec![0], 1)]);
      assert_eq!(gutters(">  a"), vec![("> ", vec![0], 1)]);
      assert_eq!(gutters(">a"), vec![(">", vec![0], 1)]);
  }

  #[test]
  fn a_fence_in_a_quote_takes_one_marker_per_quote() {
      assert_eq!(
          gutters("> ```\n> > x\n> ```"),
          vec![("> ", vec![0], 1), ("> ", vec![6], 1), ("> ", vec![12], 1)]
      );
      assert_eq!(gutters("> ```\nx"), vec![("> ", vec![0], 1)]);
  }

  #[test]
  fn an_indented_code_block_keeps_its_indent_out_of_the_gutter() {
      assert_eq!(
          gutters(">     code\n>     more"),
          vec![("> ", vec![0], 1), ("> ", vec![11], 1)]
      );
  }

  #[test]
  fn a_quote_inside_a_list_item_has_no_gutter() {
      assert_eq!(gutters("- > a\n  > b"), vec![]);
      assert_eq!(gutters("> - > a"), vec![("> ", vec![0], 1)]);
      assert_eq!(
          gutters("> - a\n>   b"),
          vec![("> ", vec![0], 1), ("> ", vec![6], 1)]
      );
  }

  #[test]
  fn a_blank_quote_line_has_a_gutter_and_a_blank_line_ends_the_quote() {
      assert_eq!(
          gutters(">a\n>\n> b"),
          vec![(">", vec![0], 1), (">", vec![3], 1), ("> ", vec![5], 1)]
      );
      assert_eq!(
          gutters("> a\n\n> b"),
          vec![("> ", vec![0], 1), ("> ", vec![5], 1)]
      );
      assert_eq!(gutters("> a\n"), vec![("> ", vec![0], 1)]);
  }

  #[test]
  fn a_heading_in_a_quote_starts_after_the_gutter() {
      assert_eq!(gutters("> # q"), vec![("> ", vec![0], 1)]);
  }

  proptest! {
      #[test]
      fn gutters_are_well_formed_on_any_text(text in any::<String>()) {
          gutters_are_well_formed(&text)?;
      }

      #[test]
      fn gutters_are_well_formed_on_block_constructs(
          fragments in prop::collection::vec(syntax_fragment(), 0..40)
      ) {
          gutters_are_well_formed(&fragments.concat())?;
      }
  }

  /// G1–G3.
  fn gutters_are_well_formed(text: &str) -> Result<(), TestCaseError> {
      let markdown = parse(text);
      let mut previous = None;
      for gutter in &markdown.gutters {
          let (start, end) = (gutter.range.start.0, gutter.range.end.0);
          let line_end = text[start..].find('\n').map_or(text.len(), |len| start + len);
          prop_assert!(start == 0 || text.as_bytes()[start - 1] == b'\n', "{gutter:?} in {text:?}");
          prop_assert!(previous.is_none_or(|previous| previous < start), "{gutter:?} in {text:?}");
          prop_assert!(start <= end && end <= line_end, "{gutter:?} in {text:?}");
          prop_assert!(1 <= gutter.depth && gutter.quotes.len() <= gutter.depth);
          prop_assert_eq!(gutter.quotes.is_empty(), start == end);
          for quote in &gutter.quotes {
              prop_assert!(gutter.range.contains(quote));
              prop_assert_eq!(text.as_bytes()[quote.0], b'>');
              let marker = markdown.spans.iter().any(|span| {
                  span.range.contains(quote) && span.styles.contains(Styles::QUOTE | Styles::MARKER)
              });
              prop_assert!(marker, "{quote:?} is no quote marker in {text:?}");
          }
          previous = Some(start);
      }
      Ok(())
  }
  ```

  `syntax_fragment` (`parse/tests.rs:621-638`) gains `Just(">".to_string())`
  and `Just("    ".to_string())`, so fragments build `>>`, lazy lines and a
  four-space `>`.
- [ ] Run `cargo test -p inkra-markdown gutters`. Expected: compile error
  `no field `gutters` on type `Markdown``.
- [ ] Implement `crates/markdown/src/gutters.rs`:

  ```rust
  //! Quote gutters: each quoted line's `>` prefix, found from the parse (markdown blocks spec, D1).

  use std::ops::Range;

  use inkra_text::ByteOffset;

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Gutter {
      /// From the line's start through its last `>` and the one space or tab after it; empty on a lazy line.
      pub range: Range<ByteOffset>,
      /// Each `>` on the line, outermost first; fewer than `depth` on a lazy line.
      pub quotes: Vec<ByteOffset>,
      /// The leading quotes that contain the line, `>` or not.
      pub depth: usize,
  }

  /// One gutter per line inside a leading quote; `quotes` holds their ranges, without a trailing
  /// `\n`, in start order.
  pub(crate) fn gutters(text: &str, quotes: &[Range<usize>]) -> Vec<Gutter> {
      let mut gutters = Vec::new();
      let mut open: Vec<&Range<usize>> = Vec::new();
      let mut rest = quotes;
      let mut start = 0;
      loop {
          if open.is_empty() {
              let Some(next) = rest.first() else {
                  break;
              };
              start = text[..next.start].rfind('\n').map_or(0, |newline| newline + 1);
          }
          let end = text[start..].find('\n').map_or(text.len(), |len| start + len);
          let entering = rest.iter().take_while(|quote| quote.start <= end).count();
          open.extend(&rest[..entering]);
          rest = &rest[entering..];
          gutters.push(gutter(text, start..end, &open));
          open.retain(|quote| end < quote.end);
          start = end + 1;
      }
      gutters
  }

  /// One `>` per open quote, outermost first, each after up to three spaces (CommonMark 5.1);
  /// the first one missing makes the line lazy from there on.
  fn gutter(text: &str, line: Range<usize>, open: &[&Range<usize>]) -> Gutter {
      let bytes = &text.as_bytes()[line.clone()];
      let mut end = 0;
      let mut quotes = Vec::new();
      for _ in open {
          let indent = bytes[end..].iter().take(3).take_while(|&&byte| byte == b' ').count();
          if bytes.get(end + indent) != Some(&b'>') {
              break;
          }
          quotes.push(ByteOffset(line.start + end + indent));
          end += indent + 1;
          end += usize::from(matches!(bytes.get(end), Some(b' ' | b'\t')));
      }
      Gutter {
          range: ByteOffset(line.start)..ByteOffset(line.start + end),
          quotes,
          depth: open.len(),
      }
  }
  ```

  `open.retain(|quote| end < quote.end)` keeps a quote exactly when it runs
  past this line: a clipped range ends at a line's end or the text's end.
- [ ] `lib.rs`: `mod gutters;` and `pub use gutters::Gutter;`.
- [ ] `parse.rs`:

  ```rust
  pub struct Markdown {
      // spans, conceals, headings unchanged
      /// Sorted by `range.start`; one per line inside a leading quote, lazy lines included.
      pub gutters: Vec<Gutter>,
  }
  ```

  In `parse`, `let mut quotes = Vec::new();` beside `headings`. In the
  `Event::Start(tag)` arm, before `open.push`:

  ```rust
                  // A quote leads its line's prefix when every ancestor is a quote; a list item breaks it.
                  let leading = matches!(tag, Tag::BlockQuote(_))
                      && current.as_ref().is_none_or(|open| open.leading);
                  if leading {
                      quotes.push(without_line_ending(text, &range));
                  }
  ```

  `Open` gains `leading: bool` (doc: "A quote whose ancestors are all
  quotes."); the `open.push` sets `leading`, the `Event::Code` literal sets
  `false`. The result gains `gutters: gutters(text, &quotes)`. New helper:

  ```rust
  /// A block's range ends after its last `\n`; a gutter must not reach the line after it.
  fn without_line_ending(text: &str, range: &Range<usize>) -> Range<usize> {
      range.start..range.end - usize::from(text[..range.end].ends_with('\n'))
  }
  ```

- [ ] Run `cargo test -p inkra-markdown`. Expected: `test result: ok.` for
  the lib tests, the new ones included.
- [ ] Failing app tests. Move `assert_x` from `editor/tests/hanging.rs:10-13`
  to `editor/tests/mod.rs` as `pub(super) fn assert_x`; `hanging.rs` imports
  it. New `crates/app/src/editor/tests/gutter.rs` (`mod gutter;` in
  `tests/mod.rs`):

  ```rust
  //! Quote gutters (markdown blocks spec, D3). Headless, a body glyph is 10.8 px, so `> ` is 21.6.

  use gpui::{Modifiers, TestAppContext, point, px};

  use super::{assert_x, at, dispatching};
  use crate::text_input::InputHost;

  const GLYPH: f32 = 0.6 * 18.;

  /// 100 chars in five-char words beside `> `: 68 fit a row (734.4 px), so the wrap falls at 65.
  fn wrapped_quote() -> String {
      format!("> {}\nbody", "aaaa ".repeat(20))
  }

  #[gpui::test]
  fn a_wrapped_quote_line_starts_every_row_at_the_gutter_edge(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, &wrapped_quote());
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              assert!(layout.shown(0).starts_with("aaaa"));
              assert_eq!(layout.row_count(), 3);
              assert_eq!(*layout.row_bounds(1).start(), at(67));
              assert_x(layout.x_for_offset(at(2)), 2. * GLYPH);
              assert_x(layout.x_for_offset(at(67)), 2. * GLYPH);
          })
          .unwrap();
  }

  #[gpui::test]
  fn nested_quotes_indent_by_both_markers(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> > x\nbody");
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              assert_eq!(layout.shown(0), "x");
              assert_x(layout.x_for_offset(at(4)), 4. * GLYPH);
          })
          .unwrap();
  }

  /// Rows 0 and 1 of the wrapped line: `x < 21.6` is the gutter's, and it lands on the row's text start.
  #[gpui::test]
  fn a_click_in_the_gutter_lands_at_the_text_start(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, &wrapped_quote());
      cx.run_until_parked();
      cx.simulate_keystrokes("cmd-down");
      let text = window
          .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
          .unwrap();
      for (x, y, expected) in [(5., 10., 2), (-5., 10., 2), (5., 38., 67)] {
          cx.simulate_click(text.origin + point(px(x), px(y)), Modifiers::none());
          let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
          assert_eq!(caret, at(expected), "({x}, {y})");
      }
  }

  #[gpui::test]
  fn down_from_a_body_line_start_lands_at_the_quote_text(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "x\n> quote");
      cx.run_until_parked();
      cx.simulate_keystrokes("cmd-up down");
      window
          .update(&mut cx, |view, _, _| assert_eq!(view.cursor(), at(4)))
          .unwrap();
  }

  /// A selection across a quote line starts at the gutter's left; a wrapped row at its edge.
  #[gpui::test]
  fn a_selection_across_a_quote_covers_its_gutter(cx: &mut TestAppContext) {
      let text = wrapped_quote();
      let (window, mut cx) = dispatching(cx, &text);
      cx.run_until_parked();
      cx.simulate_keystrokes("cmd-a");
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              let quads = layout.selection_quads(&(at(0)..at(text.len())));
              let lefts: Vec<_> = quads.iter().map(|quad| quad.bounds.origin.x).collect();
              assert_eq!(lefts[0], px(0.));
              assert_x(lefts[1], 2. * GLYPH);
              assert_eq!(lefts[2], px(0.));
          })
          .unwrap();
  }
  ```

  In `hanging.rs`, `a_heading_inside_a_quote_keeps_its_inline_conceal`
  becomes `a_heading_inside_a_quote_keeps_its_inline_conceal_after_the_gutter`:
  `shown(0)` is `"# q"`, and `x_for_offset(at(2))` is `2. * 0.6 * 18.`
  (21.6: the gutter is body size, not H1). It fails today on `shown(0)`.
- [ ] Run `cargo test -p inkra-app -- editor::tests::gutter editor::tests::hanging`.
  Expected: `FAILED`; `a_wrapped_…` on `shown(0)`, the click test at
  `(5, 10)` with `at(0)`, the selection test on `lefts[1]`.
- [ ] Implement, `prefix.rs`:

  ```rust
  //! What sits left of a line's text: a heading's hanging marker (live-preview spec, D12) or a
  //! quote's gutter (markdown blocks spec, D3).

  pub(super) enum Prefix {
      /// A heading's opening marker, left of the column.
      Margin(Hanging),
      /// A quote prefix; the line's text starts at its width on every row.
      Gutter(GutterLayout),
  }

  impl Prefix {
      pub(super) fn range(&self) -> &Range<ByteOffset> {
          match self {
              Self::Margin(hanging) => &hanging.range,
              Self::Gutter(gutter) => &gutter.range,
          }
      }

      /// Where the text starts: a margin hangs left of it, a gutter pushes it right.
      pub(super) fn indent(&self) -> Pixels {
          match self {
              Self::Margin(_) => px(0.),
              Self::Gutter(gutter) => gutter.width,
          }
      }

      /// Row 0's `x` for an offset before `range().end`; `None` for a concealed margin.
      pub(super) fn x_for_offset(&self, offset: ByteOffset) -> Option<Pixels> {
          match self {
              Self::Margin(hanging) => hanging.x_for_offset(offset),
              Self::Gutter(gutter) => {
                  Some(gutter.shaped.x_for_index(offset.0 - gutter.range.start.0))
              }
          }
      }

      /// Row 0's offset for an `x` left of `indent()`.
      pub(super) fn offset_for_x(&self, x: Pixels) -> ByteOffset {
          match self {
              Self::Margin(hanging) => hanging.offset_for_x(x),
              Self::Gutter(gutter) => gutter.offset_for_x(x),
          }
      }

      /// What paints outside the text, and at which `x`; `None` while concealed.
      pub(super) fn shown(&self) -> Option<(&ShapedLine, Pixels)> {
          match self {
              Self::Margin(hanging) => hanging
                  .shaped
                  .as_ref()
                  .map(|shaped| (shaped, -shaped.width())),
              Self::Gutter(_) => None,
          }
      }
  }

  pub(super) struct GutterLayout {
      pub(super) range: Range<ByteOffset>,
      /// The gutter's own text at the body size, in both states: the caret reads its `x` there.
      pub(super) shaped: ShapedLine,
      pub(super) width: Pixels,
  }

  impl GutterLayout {
      /// The `>`s show no glyph yet, so the whole gutter lands on the text's start.
      fn offset_for_x(&self, _: Pixels) -> ByteOffset {
          self.range.end
      }
  }
  ```

  `mod.rs`: `LineLayout.hanging` becomes `prefix: Option<Prefix>` (doc:
  "Kept out of the shaped text in both states, so no reveal re-wraps the
  line."). Add:

  ```rust
  impl LineLayout {
      fn indent(&self) -> Pixels {
          self.prefix.as_ref().map_or(px(0.), Prefix::indent)
      }
  }
  ```

  `hanging_on` becomes `prefix_on` returning `Option<&Prefix>` (row 0 only).
  `x_on_row`:

  ```rust
      fn x_on_row(line: &LineLayout, within: usize, offset: ByteOffset) -> Pixels {
          let offset = offset.clamp(line.range.start, line.range.end);
          if let Some(x) = Self::prefix_on(line, within)
              .filter(|prefix| offset < prefix.range().end)
              .and_then(|prefix| prefix.x_for_offset(offset))
          {
              return x;
          }
          let index = line.display.to_display(offset).0;
          line.indent() + line.wrapped.unwrapped_layout.x_for_index(index)
              - line.rows[within].start_x
      }
  ```

  `offset_on_row`: the split becomes `prefix.filter(|prefix| x <
  prefix.indent())` calling `prefix.offset_for_x(x)`; the text lookup becomes
  `closest_index_for_x(x - line.indent() + target.start_x)`; the last line
  becomes `prefix.map_or(offset, |prefix| offset.max(prefix.range().end))`.
  Its comment: "Row 0 splits at the text's start: left of it is the
  prefix's, and the text never maps into the prefix (D12)."

  `paint.rs`: each line paints at `line_origin + point(line.indent(), px(0.))`;
  the marker paint reads `line.prefix.as_ref().and_then(Prefix::shown)` and
  paints at `line_origin + point(x, px(0.))`.

  `shape.rs`: a third cursor, `let mut gutters = markdown.gutters.as_slice();`.
  Per line, after the conceal split:

  ```rust
              let own = gutters
                  .iter()
                  .take_while(|gutter| gutter.range.start <= range.end)
                  .count();
              let (line_gutters, next) = gutters.split_at(own);
              gutters = next;
              let prefix = match line_gutters.first() {
                  Some(gutter) => {
                      let shaped = show(&gutter.range, text, line_spans, marked, body_size, &mut styles, window);
                      Some(Prefix::Gutter(GutterLayout {
                          range: gutter.range.clone(),
                          width: shaped.width(),
                          shaped,
                      }))
                  }
                  None => opening.map(|range| {
                      // today's `Hanging` construction, unchanged
                      Prefix::Margin(Hanging { shaped: revealed.then(|| show(..)), range })
                  }),
              };
              let display = display_line(&range, line_conceals, prefix.as_ref().map(Prefix::range), selection);
              let indent = prefix.as_ref().map_or(px(0.), Prefix::indent);
              // ...
                  .shape_text(shown.clone(), font_size, &runs, wrap_width.map(|width| (width - indent).max(px(0.))), None)
  ```

  `display` moves below `prefix`; `shown` and `runs` follow it, as today.
  `display_line`:

  ```rust
  /// The prefix never enters the shaped text, so no reveal can re-wrap its line.
  fn display_line(
      range: &Range<ByteOffset>,
      conceals: &[Conceal],
      prefix: Option<&Range<ByteOffset>>,
      selection: &Range<ByteOffset>,
  ) -> DisplayLine {
      let end = prefix.map_or(range.start, |prefix| prefix.end);
      // A lazy line's gutter hides nothing.
      let hidden = prefix.filter(|prefix| !prefix.is_empty()).map(|prefix| (prefix.clone(), ""));
      // A marker starting before the prefix's end is the prefix's own (plan, Decision 4).
      let active = conceals
          .iter()
          .filter(|conceal| conceal.marker.start >= end && is_active(conceal, selection))
          .map(|conceal| (conceal.marker.clone(), conceal.replacement));
      DisplayLine::new(range.clone(), hidden.into_iter().chain(active))
  }
  ```

  `show` is unchanged: a gutter's range starts its line, as a margin's does.
- [ ] Run `cargo test -p inkra-markdown && cargo test -p inkra-app editor::`.
  Expected: `test result: ok.` for both; every `hanging.rs` test still passes.
- [ ] Commit: `Quotes: a gutter keeps the > out of the text and indents every row`.

## Task 3: a lazy line starts at the gutter above

Task 2 gives `b` in `> > a\nb` an empty gutter, so it starts at 0. G6 wants
43.2.

- [ ] Failing tests in `gutter.rs`:

  ```rust
  #[gpui::test]
  fn a_lazy_line_starts_at_the_gutter_above(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> > a\nb");
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              assert_x(layout.x_for_offset(at(6)), 4. * GLYPH);
          })
          .unwrap();
  }

  /// `b` has one `>` of its two levels: its text still starts under `a`.
  #[gpui::test]
  fn a_line_short_of_its_depth_starts_at_the_gutter_above(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> > a\n> b");
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              assert_eq!(layout.shown(1), "b");
              assert_x(layout.x_for_offset(at(8)), 4. * GLYPH);
          })
          .unwrap();
  }
  ```

- [ ] Run `cargo test -p inkra-app editor::tests::gutter`. Expected:
  `FAILED`, `0 != 43.2` and `21.6 != 43.2`.
- [ ] Implement. `prefix.rs`:

  ```rust
  /// A gutter's text start, carried to lazy lines (spec D3).
  #[derive(Clone, Copy, Default)]
  pub(super) struct Geometry {
      pub(super) width: Pixels,
  }

  impl GutterLayout {
      /// A gutter holding all its `>` measures itself; a lazy one takes `carried`, the last measured.
      pub(super) fn new(gutter: &Gutter, shaped: ShapedLine, carried: &mut Geometry) -> Self {
          if gutter.quotes.len() == gutter.depth {
              *carried = Geometry { width: shaped.width() };
          }
          Self { range: gutter.range.clone(), shaped, width: carried.width }
      }
  }
  ```

  `shape.rs`: `let mut carried = Geometry::default();` beside the cursors;
  the gutter arm calls `GutterLayout::new(gutter, shaped, &mut carried)`.
- [ ] Run `cargo test -p inkra-app editor::tests::gutter`. Expected: `ok`.
- [ ] Commit: `Quotes: a lazy line takes the gutter above it`.

## Task 4: a line's `>`s conceal together and paint in the gutter

- [ ] Failing markdown tests. In `parse/tests/gutters.rs`, import `concealed`
  from `super`:

  ```rust
  #[test]
  fn a_lines_markers_conceal_together() {
      assert_eq!(concealed("> a"), vec![(">", ">", "")]);
      assert_eq!(
          concealed("> > x"),
          vec![(">", "> >", ""), (">", "> >", "")]
      );
      assert_eq!(concealed("  > a"), vec![(">", "  >", "")]);
      assert_eq!(concealed("> a\nb"), vec![(">", ">", "")]);
      assert_eq!(concealed("- > a"), vec![]);
  }

  /// The gap rule's prefix skip consumes a superset of a gutter, so its markers start at or past the gutter's end.
  #[test]
  fn a_strong_across_quote_lines_keeps_its_markers_out_of_the_gutter() {
      assert_eq!(
          concealed("> **a\n> b**"),
          vec![
              (">", ">", ""),
              ("**", "**a\n> b**", ""),
              (">", ">", ""),
              ("**", "**a\n> b**", ""),
          ]
      );
  }
  ```

  `gutters_are_well_formed` gains G4:

  ```rust
          let inside: Vec<_> = markdown
              .conceals
              .iter()
              .filter(|conceal| gutter.range.contains(&conceal.marker.start))
              .map(|conceal| {
                  prop_assert_eq!(conceal.marker.end.0, conceal.marker.start.0 + 1);
                  Ok(conceal.marker.start)
              })
              .collect::<Result<_, TestCaseError>>()?;
          prop_assert_eq!(&inside, &gutter.quotes);
  ```

  Updated expectations in `parse/tests.rs` (they fail until the conceals
  exist):
  - `:356` `concealed("> # q\n")` → `[(">", ">", ""), ("# ", "# q", "")]`;
  - `:444` `> [a\n> b](u)` → `[(">", ">", ""), ("[", …), (">", ">", ""), ("](u)", …)]`;
  - `:452` `> [a\n> ](u)` → the same shape;
  - `:479` `concealed("> - a")` → `[(">", ">", ""), ("-", "-", "•")]`;
  - `:493` `quotes_and_escapes_conceal_nothing_themselves` becomes
    `escapes_conceal_nothing_and_a_quote_only_its_markers`:
    `> **q**` → `[(">", ">", ""), ("**", "**q**", ""), ("**", "**q**", "")]`.
- [ ] Run `cargo test -p inkra-markdown`. Expected: `FAILED`, the six tests
  above and the G4 proptests.
- [ ] Implement in `gutters.rs`:

  ```rust
  /// One conceal per `>`, all with the element from the gutter's start through its last `>`, so a
  /// line's `>`s reveal together and a caret after the last one's space reveals none.
  pub(crate) fn quote_conceals(gutters: &[Gutter]) -> impl Iterator<Item = Conceal> + '_ {
      gutters.iter().flat_map(|gutter| {
          let last = gutter.quotes.last().map_or(gutter.range.start, |last| ByteOffset(last.0 + 1));
          let element = gutter.range.start..last;
          gutter.quotes.iter().map(move |&quote| Conceal {
              element: element.clone(),
              marker: quote..ByteOffset(quote.0 + 1),
              replacement: "",
          })
      })
  }
  ```

  In `parse`, at the end:

  ```rust
      let gutters = gutters(text, &quotes);
      let mut conceals = out.conceals;
      conceals.extend(quote_conceals(&gutters));
      // Two sorted runs: the stable sort merges them in one linear pass.
      conceals.sort_by_key(|conceal| conceal.marker.start);
  ```

- [ ] Run `cargo test -p inkra-markdown`. Expected: `ok`.
- [ ] Failing app tests in `gutter.rs` (its imports gain `gpui::VisualTestContext`). First a test accessor in
  `layout/mod.rs`, beside `shown`:

  ```rust
      /// Whether line `line`'s prefix paints its source text.
      #[cfg(test)]
      pub(crate) fn prefix_painted(&self, line: usize) -> bool {
          self.lines[line].prefix.as_ref().and_then(Prefix::shown).is_some()
      }
  ```

  ```rust
  /// `cmd-up` puts the caret on the `>`: it paints, and the text keeps every row and every x (G5).
  #[gpui::test]
  fn revealing_a_quote_marker_moves_no_text(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, &wrapped_quote());
      cx.run_until_parked();
      let geometry = |cx: &mut VisualTestContext| {
          window
              .update(cx, |view, _, _| {
                  let layout = view.input.last_layout().unwrap();
                  let tops: Vec<_> = (0..=layout.row_count())
                      .map(|row| layout.top_for_row(row))
                      .collect();
                  let xs = (layout.x_for_offset(at(2)), layout.x_for_offset(at(67)));
                  (layout.prefix_painted(0), tops, xs)
              })
              .unwrap()
      };
      cx.simulate_keystrokes("cmd-down");
      let (painted, concealed, xs) = geometry(&mut cx);
      assert!(!painted);
      cx.simulate_keystrokes("cmd-up");
      let (painted, revealed, revealed_xs) = geometry(&mut cx);
      assert!(painted);
      assert_eq!((revealed, revealed_xs), (concealed, xs));
  }

  /// `x = 13` is nearer the `>`'s end (10.8) than the space's (21.6); `x = 3` nearer its start.
  #[gpui::test]
  fn a_click_on_a_revealed_quote_marker_lands_on_it(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> quote\nbody");
      cx.run_until_parked();
      cx.simulate_keystrokes("cmd-up");
      let text = window
          .update(&mut cx, |view, _, _| view.text_bounds().unwrap())
          .unwrap();
      for (x, expected) in [(13., 1), (3., 0)] {
          cx.simulate_click(text.origin + point(px(x), px(10.)), Modifiers::none());
          let caret = window.update(&mut cx, |view, _, _| view.cursor()).unwrap();
          assert_eq!(caret, at(expected), "x = {x}");
      }
  }

  #[gpui::test]
  fn a_caret_after_the_gutter_leaves_it_concealed(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> > quote\nbody");
      cx.run_until_parked();
      cx.simulate_keystrokes("cmd-up right right right right");
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              assert_eq!(view.cursor(), at(4));
              assert!(!layout.prefix_painted(0));
              assert_x(layout.caret_bounds(at(3)).origin.x, 3. * GLYPH);
          })
          .unwrap();
  }
  ```

  `a_click_in_the_gutter_lands_at_the_text_start` (Task 2) still passes: its
  first click leaves the caret at 2, which touches no `>` (G7).
- [ ] Run `cargo test -p inkra-app editor::tests::gutter`. Expected: `FAILED`,
  `assert!(painted)` and the click at 13 landing on `at(2)`. The third test
  passes already: it guards the shared element's end.
- [ ] Implement. `GutterLayout` gains `revealed`, and `new` takes it:

  ```rust
  pub(super) struct GutterLayout {
      pub(super) range: Range<ByteOffset>,
      pub(super) shaped: ShapedLine,
      pub(super) width: Pixels,
      /// Whether the line's `>`s paint (a caret touches one) rather than hide (spec D2).
      pub(super) revealed: bool,
  }

  impl GutterLayout {
      /// A gutter holding all its `>` measures itself; a lazy one takes `carried`, the last measured.
      pub(super) fn new(
          gutter: &Gutter,
          shaped: ShapedLine,
          carried: &mut Geometry,
          revealed: bool,
      ) -> Self {
          if gutter.quotes.len() == gutter.depth {
              *carried = Geometry { width: shaped.width() };
          }
          Self { range: gutter.range.clone(), shaped, width: carried.width, revealed }
      }
  }
  ```

  `offset_for_x` and `Prefix::shown`:

  ```rust
      /// A revealed gutter maps `x` through its shape (as D12's margin); a concealed one lands on the text's start.
      fn offset_for_x(&self, x: Pixels) -> ByteOffset {
          let index = if self.revealed {
              self.shaped.closest_index_for_x(x)
          } else {
              self.shaped.len
          };
          ByteOffset(self.range.start.0 + index)
      }
  ```

  ```rust
              Self::Gutter(gutter) => gutter.revealed.then_some((&gutter.shaped, px(0.))),
  ```

  `shape.rs`, the gutter arm:

  ```rust
                      // A line's `>`s share one element, so one test decides them all.
                      let revealed = line_conceals.iter().any(|conceal| {
                          conceal.marker.end <= gutter.range.end && !is_active(conceal, selection)
                      });
  ```

  and the call becomes `GutterLayout::new(gutter, shaped, &mut carried, revealed)`.

  `self.shaped.len` is the gutter's own text length; on a lazy line with a
  shorter own text than the carried width, `x` between the two still lands on
  `range.end`.
- [ ] Run `cargo test -p inkra-markdown && cargo test -p inkra-app editor::`.
  Expected: `ok` for both.
- [ ] Commit: `Quotes: a line's > markers conceal together and paint in the gutter`.

## Task 5: bars, theme, and the scene

- [ ] Failing tests in `gutter.rs`:

  ```rust
  use gpui::{Bounds, Pixels};

  fn bar(x: f32, top: f32, bottom: f32) -> Bounds<Pixels> {
      Bounds::from_corners(point(px(x), px(top)), point(px(x + 2.), px(bottom)))
  }

  /// The wrapped line's bar covers both its rows and meets the next line's.
  #[gpui::test]
  fn a_quote_bar_runs_from_its_line_top_to_the_next_line(cx: &mut TestAppContext) {
      let text = format!("> {}\n> b\nbody", "aaaa ".repeat(20));
      let (window, mut cx) = dispatching(cx, &text);
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              let bars: Vec<_> = layout.quote_bars().collect();
              assert_eq!(bars, [bar(0., 0., 56.), bar(0., 56., 84.)]);
          })
          .unwrap();
  }

  #[gpui::test]
  fn nested_quotes_draw_a_bar_per_level_and_a_lazy_line_keeps_them(cx: &mut TestAppContext) {
      let (window, mut cx) = dispatching(cx, "> > a\nb\n\nbody");
      cx.run_until_parked();
      window
          .update(&mut cx, |view, _, _| {
              let layout = view.input.last_layout().unwrap();
              let lefts: Vec<_> = layout.quote_bars().map(|bar| f32::from(bar.origin.x)).collect();
              assert_eq!(lefts.len(), 4);
              for (left, expected) in lefts.iter().zip([0., 2. * GLYPH, 0., 2. * GLYPH]) {
                  assert!((left - expected).abs() < 0.01, "{lefts:?}");
              }
          })
          .unwrap();
  }
  ```

  `2.` is `QUOTE_BAR_WIDTH`, written out as `tests/mod.rs`'s `geometry`
  module does.
- [ ] Run `cargo test -p inkra-app editor::tests::gutter`. Expected: compile
  error, no method `quote_bars`.
- [ ] Implement. `theme.rs`, after `quote_text`:

  ```rust
  /// One per quote level, down the gutter (markdown blocks spec, D3).
  pub(crate) fn quote_bar() -> Rgba {
      rgb(0x4a4a4a)
  }

  pub(crate) const QUOTE_BAR_WIDTH: Pixels = px(2.);
  ```

  `prefix.rs`: `Geometry` gains `bars: Rc<[Pixels]>` and drops `Copy`
  (`Rc<[T]>` is `Default`). `GutterLayout.width` becomes `geometry:
  Geometry`; `indent` reads `geometry.width`. In `new`:

  ```rust
          if gutter.quotes.len() == gutter.depth {
              let start = gutter.range.start.0;
              *carried = Geometry {
                  width: shaped.width(),
                  bars: gutter.quotes.iter().map(|quote| shaped.x_for_index(quote.0 - start)).collect(),
              };
          }
          Self { range: gutter.range.clone(), shaped, revealed, geometry: carried.clone() }
  ```

  The signature of `new` is unchanged from Task 4 (`revealed: bool` last).
  `Prefix` gains `bars`, and `indent`'s gutter arm reads the geometry:

  ```rust
      /// Only a gutter draws bars (spec D3).
      pub(super) fn bars(&self) -> &[Pixels] {
          match self {
              Self::Margin(_) => &[],
              Self::Gutter(gutter) => &gutter.geometry.bars,
          }
      }
  ```

  ```rust
              Self::Gutter(gutter) => gutter.geometry.width,
  ```

  `paint.rs`:

  ```rust
      /// Each bar at its `>`'s `x`, from its line's top to the next line's, so a quote reads as one bar.
      pub(crate) fn quote_bars(&self) -> impl Iterator<Item = Bounds<Pixels>> + '_ {
          let bottoms = self.lines.iter().skip(1).map(|line| line.top).chain([self.height]);
          self.lines.iter().zip(bottoms).flat_map(|(line, bottom)| {
              let bars = line.prefix.as_ref().map_or(&[][..], Prefix::bars);
              bars.iter().map(move |&x| {
                  Bounds::from_corners(
                      point(x, line.top),
                      point(x + theme::QUOTE_BAR_WIDTH, bottom),
                  )
              })
          })
      }
  ```

  `paint` starts with `for bar in self.quote_bars() { window.paint_quad(fill(bar
  + origin, theme::quote_bar())); }`, so a revealed `>` paints over its bar.
- [ ] Run `cargo test -p inkra-app editor::`. Expected: `ok`.
- [ ] The scene. New `crates/app/src/shot/scenes/markdown_blocks.rs`:

  ```rust
  use gpui::WindowHandle;

  use crate::shell::AppShell;
  use crate::shot::stage::Stage;

  /// Checked against pulldown-cmark 0.13.4: the lazy line joins the nested quote, the heading and the list the outer one.
  const QUOTES: &str = "A paragraph before the quotes.\n\
                        > A quote over two lines,\n\
                        > the second one here.\n\
                        >\n\
                        > A quote line long enough to wrap: its second row starts at the gutter's edge, clear of the bar.\n\
                        > > A nested quote, two bars,\n\
                        > > that runs on\n\
                        with a lazy line.\n\
                        > # A heading in a quote\n\
                        > - a list in a quote\n\
                        \n\
                        Body text after the quotes.";

  pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
      let window = stage.open_note(QUOTES);
      stage.focus(window);
      stage.keys(window, "cmd-down");
      window
  }

  /// `home` goes to the line's start, on its `>`.
  pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
      let window = stage.open_note(QUOTES);
      stage.focus(window);
      stage.keys(window, "cmd-up down home");
      window
  }
  ```

  `scenes.rs`: `mod markdown_blocks;` in the module list, and at the end of
  `CATALOGUE`:

  ```rust
      Scene {
          name: "markdown-blocks",
          shows: "quotes: bars per level, every row at the gutter's edge, a lazy line, a heading and a list in a quote",
          open: markdown_blocks::concealed,
      },
      Scene {
          name: "markdown-blocks-revealed",
          shows: "the caret on a quote's first line: its > paints in the gutter, no text moved",
          open: markdown_blocks::revealed,
      },
  ```

- [ ] Render and look: `cargo run -p inkra-app --features shot --example shot
  -- markdown-blocks`, then `-- markdown-blocks-revealed`, `-- editor-markdown`,
  `-- editor-headings`, `-- editor-heading-revealed`. Expected: one line each,
  `markdown-blocks  2000x1280  target/shots/markdown-blocks.png` and so on.
  Read each PNG. Check: two bars on the nested and lazy lines, one elsewhere;
  bars unbroken between lines; the wrapped line's second row aligned with its
  first; the heading's text start aligned with the body lines; in the revealed
  shot, `>` dimmed over its bar and the text where it was; the D12 margin
  unchanged in the heading scenes. Bar colour, width or a blurry fractional
  `x` get tuned in `theme.rs` here, and the tests follow any width change.
- [ ] Commit: `Quotes: a bar per level down the gutter; the markdown-blocks scene`.

## Task 6: measure

- [ ] `git apply target/perf-quotes.patch`; run the Task 0 command three
  times; record the medians in `target/perf-mb-s1-1.txt`; `git apply -R
  target/perf-quotes.patch`; `git status --short` shows only intended files.
  Expected cost: one cached `shape_line` per quoted line per shape (spec D3).
  If the median grows by more than 20 %, profile before Task 7 and report;
  do not optimise blind (CLAUDE.md).
- [ ] `cargo bench -p inkra-markdown -- --quick 2>&1 | tee
  target/bench-mb-s1-1.txt`; compare with Task 0. Not a gate.
- [ ] `cargo mutants -f crates/markdown/src/gutters.rs -f
  crates/markdown/src/parse.rs`. Expected: `0 missed`. An equivalent mutant
  gets a reason in `.cargo/mutants.toml`, matched by column, as the file's
  other entries. Likely candidate: `quote.start <= end` to `<` in `gutters`
  (a quote's `>` is a byte of its first line, so a quote never starts at the
  line's end). Never `--in-place`.
- [ ] Optional: `cargo mutants -p inkra-app --test-package inkra-app -f
  crates/app/src/editor/layout/prefix.rs`.

## Task 7: docs

- [ ] Spec: "As built (S1)" after Slices: the ten Spec corrections above;
  the before/after medians and bench lines; the mutants count. Deviations
  from Zed gains the `next_indent` row (correction 9). Deferred gains "A
  quote inside a list item gets a gutter | A quote in a list is reported" and
  "A tab before a nested `>` counts toward its indent | One is reported"
  (Risks).
- [ ] `CLAUDE.md`, Docs list, after the live-preview entry:
  "`docs/superpowers/specs/2026-09-30-markdown-blocks-design.md` — quote
  gutters and bars, code backgrounds, task boxes".
- [ ] `.claude/rules/editor.md`, Layout, a bullet: "A line's `Prefix` sits
  outside its shaped text: a heading's `Margin` hangs at negative `x`, a
  quote's `Gutter` moves every row right by its width. Conversions add
  `LineLayout::indent`; nothing else adds a width."
- [ ] Commit: `Record markdown blocks S1`.

## Task 8: gate

- [ ] `./scripts/check.sh`. Expected last line: `==> all checks passed`.
- [ ] `./scripts/test-names.sh | diff target/names-mb-s1-0.txt -`. Expected:
  additions only, plus two renames (`<` old, `>` new):
  `a_heading_inside_a_quote_keeps_its_inline_conceal` and
  `quotes_and_escapes_conceal_nothing_themselves`.
- [ ] Re-render `markdown-blocks` and `markdown-blocks-revealed` after the
  last change and read both.

## Deliberately not done

- No gutter for a quote inside a list item (correction 1).
- No `Task` in `Gutter`, no task boxes: S3.
- No fence conceals, no code background, no paint reorder: S2.
- No hanging indent for a plain list item: the spec defers it.
- A heading's `#` inside a quote still conceals inline and re-wraps on reveal
  (D12's accepted limit, unchanged).
- A caret at a quoted line's start sits in the gutter, and typing there
  breaks the quote, as typing before a heading's `#` does. Home keeps D12's
  rule.

## Risks

- **Baselines in a quoted heading.** The revealed `>` is body size in the
  heading's 44 px row, so its baseline sits below the heading text's. The
  revealed scene shows how far; D12 accepted a smaller gap.
- **Tabs before a nested `>`.** The walk counts up to three spaces, never a
  tab. pulldown expands `> \t> b` to a nested quote; the gutter then holds one
  `>` of two, and the second shows as dimmed text. Rare; deferred.
- **Fractional bar `x`.** Bars sit at a shaped glyph's `x`, often fractional;
  a 2 px quad there may render soft. The scene decides; rounding to a device
  pixel is a one-line change in `GutterLayout::new`.
- **The shared element (correction 2) is a spec change.** A red/blue review
  may prefer one element per `>`; that costs a shape per revealed `>` in the
  gutter.
- **`sort_by_key` on every parse.** Linear on two runs by the std docs; the
  bench in Task 6 confirms it.
- **The bar under a selection.** Until S2 reorders the paint, a bar covers a
  2 px strip of selection in the gutter.

## Verification

```
./scripts/check.sh
./scripts/test-names.sh | diff target/names-mb-s1-0.txt -
cargo test -p inkra-markdown gutters
cargo test -p inkra-app -- editor::tests::gutter editor::tests::hanging
cargo mutants -f crates/markdown/src/gutters.rs -f crates/markdown/src/parse.rs
cargo run -p inkra-app --features shot --example shot -- markdown-blocks
cargo run -p inkra-app --features shot --example shot -- markdown-blocks-revealed
```
