# Performance batch 2: measure, then change

Status: **in progress 2026-11-02.** S1 records the baseline and lands item 3.
S2 corrects the item 1 probe, reverses its drop, and carries shaped lines.
S3 carries them across a change in line count.
S4 paints only the lines and quote bars inside the content mask.
Later slices add their own `### As built (Sx)` under the item they change.

The 2026-11-02 Rust audit listed ten per-frame and per-edit costs. CLAUDE.md
says "Performance is a requirement, measured". This spec measures each cost
before any change. An item whose measurement shows no real cost is dropped,
with its number and its reason.

## Method

- Machine: Apple M5 Pro (`sysctl -n machdep.cpu.brand_string`), macOS, release mode.
- Items 1, 2 and 9 need the real platform and CoreText. `#[gpui::test]` uses
  `NoopTextSystem`, which hides shaping cost. A throwaway probe on the shot
  harness measures them. The probe is never committed. Its source is in the
  appendix, so a later slice can re-apply it.
- Probe run: `cargo run --release -p inkra-app --features shot --example shot -- probe-perf`,
  three runs, the median of each column. Item 9 reads the `margin_width`
  lines the probe prints on stderr between its `idle-begin 50` and `idle-end 50` markers.
- Probe protocol, per size (5, 50 and 500 KB of one repeated 1 KB sample):
  open the note, focus it, press `cmd-up`, then run four series of 20.
  - warm-x and warm-enter: draw, start the clock, press the key, draw, stop the clock.
  - cold-x and cold-enter: the same, after three draws instead of one. gpui's
    `LineLayoutCache` keeps two frames, so it is empty after two frames that shape nothing.
  - idle: the mean of 20 draws with no input.
  - typed-warm-x and typed-cold-x (added in S2): start the clock, press `x`, stop the
    clock, after zero or two draws. The key's own settle draws once.
- The warm series are not warm. In test mode gpui draws a dirty window when
  effects flush, so the key already draws once. The explicit draw after the key
  is a second frame that shapes nothing, and the next series' draw is a third.
  Every warm-x and warm-enter key therefore finds an empty cache. Only typed-warm-x is warm.
- The sample is plain prose, a `#` heading, a setext heading, emphasis, bold,
  code, a wikilink, a three-line quote, a task and a 300-char wrapping paragraph.
- Items 4, 5, 6, 7, 8 and 10 take the audit's `cargo bench` and counted-test
  figures. S1 did not re-run them; the slice that changes one re-runs its
  bench first and records before and after.
- Bench commands: `cargo bench -p inkra-text -- --quick`, `cargo bench -p inkra-markdown -- --quick`.

## Decisions

| item | measured cost | keep/drop | slice |
|---|---|---|---|
| 1 cold reshape after an idle redraw | typed-cold-x 2.512 ms vs typed-warm-x 1.093 ms at 50 KB (2.3x) | keep | S2, S3 |
| 2 paint only the visible lines | idle draw 0.163 ms at 50 KB, 1.310 ms at 500 KB (8.0x for 10x the text) | keep | S4 |
| 3 `line_for_offset` by bisection | O(lines) per lookup, several lookups per frame | keep | S1 |
| 4 UTF-16 conversions from byte 0 | `to_utf16` near the end 15.3 us / 154 us / 1.57 ms at 1k / 10k / 100k lines | keep | not in S1 |
| 5 `reveal::touched` scans twice | one lockstep pass already | drop | none |
| 6 anchor slots never reused | +0.7 us per edit at 3000 tombstones | drop | none |
| 7 sidebar builds 3 Strings per row | about 4 us per frame, constant in vault size | drop | none |
| 8 `Layout::new` 2 allocations per note | 1.67 ms of 63.17 ms at 50k notes (2.6%), on a background job | drop | none |
| 9 `margin_width` every frame | 1.14 us per call at 50 KB (limit 50 us) | drop | none |
| 10 `recent` SipHash | `empty, 20 recent` 315 us on one core; 50k SipHash lookups 0.269 ms | keep | not in S1 |

## Item 1: cold reshape after an idle redraw

Audit claim: gpui's line layout cache lives one frame (`shape.rs:98`), so a
redraw after an idle frame reshapes every visible line.

Measured (median of 3 runs, ms per key plus draw):

| size | warm-x | cold-x | warm-enter | cold-enter |
|---|---|---|---|---|
| 5KB | 1.912 | 1.838 | 1.752 | 1.732 |
| 50KB | 2.744 | 2.719 | 2.643 | 2.599 |
| 500KB | 16.032 | 15.722 | 15.889 | 15.841 |

Guard: drop the item if cold-x is at most 1.2x warm-x at 50 KB. It is 0.99x.
S1 dropped the item on that number. S2 found the number wrong: both series
are cold (see Method). `shape_document` hands every line to `shape_text` on each
reshape, and only gpui's cache keeps an unchanged line off CoreText.

Corrected (S2, before any code change, median of 3 runs, ms):

| size | typed-warm-x | typed-cold-x | ratio |
|---|---|---|---|
| 5KB | 0.318 | 1.734 | 5.5x |
| 50KB | 1.093 | 2.512 | 2.3x |
| 500KB | 13.356 | 14.486 | 1.08x |

Kept: a keystroke after an idle pause costs 2.3x a keystroke while typing at 50 KB.
In the app a caret-blink frame shapes nothing, so two blinks empty the cache.

The 13 ms an edit costs at 500 KB is mostly not shaping: warm and cold differ
by 8%. Its cause is a separate question, outside this batch.

### As built (S2)

`shape_document` takes `LayoutInputs` and `previous: Option<&DocumentLayout>`.
Line `i` reuses `previous`'s line `i` shape (`Rc<LineShape>`) when every
`shape_text` argument is equal: text, font size, runs and wrap width.
`line_clamp` is always `None`. An argument added later joins the key.
Prefix shapes (`show`, gutter prefixes) are not carried; gpui's cache serves them.

- The editor passes its last layout: `reshape`, the measure closure and the
  prepaint fallback. `NameField` passes `None`: it reshapes every prepaint,
  which keeps gpui's cache warm, and it holds one line.
- A line reuses a shape only when its inputs match, so a previous layout from
  another note or another width only misses the carry; it is never wrong.
- A failed shape stores a default `WrappedLine` under the requested text, as before.
- S2 handles only the same-index candidate. An edit that changes the line
  count misses every line after it; S3 adds that candidate.
- The carry pushed `shape_document` past clippy's `too_many_lines`. The lookup
  is `DocumentLayout::carried`, and the heading cursor moved to `line_heading`, beside `line_items`.

Measured (release probe, median of 3 runs, ms):

| size | series | before | after |
|---|---|---|---|
| 5KB | warm-x | 1.890 | 0.501 |
| 5KB | cold-x | 1.834 | 0.440 |
| 5KB | typed-warm-x | 0.318 | 0.232 |
| 5KB | typed-cold-x | 1.734 | 0.303 |
| 5KB | warm-enter | 1.732 | 1.736 |
| 5KB | cold-enter | 1.699 | 1.658 |
| 50KB | warm-x | 2.731 | 1.221 |
| 50KB | cold-x | 2.661 | 1.188 |
| 50KB | typed-warm-x | 1.093 | 0.880 |
| 50KB | typed-cold-x | 2.512 | 0.949 |
| 50KB | warm-enter | 2.601 | 2.648 |
| 50KB | cold-enter | 2.570 | 2.591 |
| 500KB | warm-x | 15.916 | 13.219 |
| 500KB | cold-x | 16.125 | 14.013 |
| 500KB | typed-warm-x | 13.356 | 12.308 |
| 500KB | typed-cold-x | 14.486 | 13.128 |
| 500KB | warm-enter | 16.066 | 17.859 |
| 500KB | cold-enter | 15.818 | 17.622 |

Idle draws did not move (0.061, 0.157 and 1.289 ms after).

Acceptance:

- cold-x at 50 KB after, 1.188 ms, is at most 1.1 × warm-x at 50 KB before (3.004 ms).
  On the corrected series, typed-cold-x after, 0.949 ms, is at most 1.1 ×
  typed-warm-x before (1.202 ms): a key after a pause now costs what a key while typing did.
- warm-x after is below warm-x before at every size, and so is typed-warm-x.

Regression: `enter` at 500 KB costs 11% more (16.066 to 17.859 ms warm). An
enter changes the line count, so every line after it misses the carry and pays
the comparison, an `Rc` and the kept runs on top of the old cost. S3's
changed-line-count candidate targets this case.

Memory: each line keeps its `Vec<TextRun>`, 144 bytes per run
(`size_of::<TextRun>()`), plus an `Rc` header. The text is shared with the
`WrappedLine`: `shape_text` keeps a one-line `SharedString` as given. A 1000-line
note with two runs per line keeps about 0.3 MB more.

Fonts: gpui's cache drops its entries when the font generation changes. A
carried shape does not see that. Inkra has no runtime `add_fonts`, so no font
can arrive after a layout today. If one does, carried lines keep the old
fallback until their inputs change.

Shots: all 54 scenes are byte-identical before and after (`cmp`).

### As built (S3)

`candidates` yields up to two lines of `previous` per new line: the one at the
same index (S2), then the one the edit moved to the new line's start. After one
contiguous edit, every line below it starts `previous length - length` bytes
from where it started, so the moved candidate is the previous line whose
`range.start` is `start + previous length - length`. `binary_search_by_key` on
`range.start` finds it in O(log lines) per line. `Buffer::line_count` would add
a whole-text scan per keystroke, which CLAUDE.md forbids. `carried` takes the
first candidate whose `shaped_from` holds, so a wrong candidate costs one shape
and never a wrong layout. A target below zero has no candidate. When the line
count does not change, both candidates can be the same line; the second
comparison is then redundant and harmless.

Several separate edits (a reload diff with many hunks) carry only the lines
before the first hunk and after the last. That is correct and only slower.

`LineShape::new` takes `&str` and builds the `SharedString` itself. Its caller
had to stay under clippy's `too_many_lines` after the carry call grew by a few lines.

Tests: `a_newline_hands_only_its_own_line_to_shape_text` and
`joining_two_lines_hands_only_the_joined_line_to_shape_text` open ten distinct
lines, so a same-index match cannot pass by coincidence. Both count one line
handed to `shape_text`. Without the moved candidate they count 10 and 9. With the
arithmetic flipped to `start + length - previous length` they count 9 and 10.

Measured (release probe, median of 3 runs, ms):

| size | series | before (S2) | after |
|---|---|---|---|
| 5KB | warm-enter | 1.736 | 0.322 |
| 5KB | cold-enter | 1.658 | 0.278 |
| 50KB | warm-enter | 2.648 | 1.081 |
| 50KB | cold-enter | 2.591 | 1.044 |
| 500KB | warm-enter | 17.859 | 13.443 |
| 500KB | cold-enter | 17.622 | 14.166 |

The S2 regression is gone: `enter` at 500 KB costs 13.443 ms warm, below the
16.066 ms it cost before S2. The other series did not move (50 KB warm-x 1.236,
cold-x 1.198, typed-cold-x 0.971; idle 0.059, 0.155 and 1.285 ms).

Acceptance: cold-enter at 50 KB, 1.044 ms, is at most 1.1 × warm-x at 50 KB
after S2 (1.188 ms, so 1.307 ms). It is 0.88x.

Shots: all 55 scenes are byte-identical to S2's (`cmp`). A scene shapes with no
previous layout, so it never reaches the carry; `markdown-blocks` was viewed.
`cargo mutants` does not cover `inkra-app`, so none ran.

## Item 2: paint only the visible lines

Audit claim: `paint.rs:14` paints the whole document.

Measured (median of 3 runs, ms per idle draw):

| size | idle |
|---|---|
| 5KB | 0.062 |
| 50KB | 0.163 |
| 500KB | 1.310 |

Guard: drop the item if idle at 500 KB is under 2x idle at 50 KB. It is 8.0x.
Kept: an idle frame costs time proportional to the document. S4 changes it.

### As built (S4)

`DocumentLayout::paint` reads `window.content_mask().bounds`. The div that owns
the scroll pushes that mask, so it holds the scrolled viewport. `visible_lines`
turns the mask into layout space (`mask.top() - origin.y`) and finds the lines
with `partition_point` on `top`, one extra line on each side because a glyph can
overhang its row. `bars_of` cuts the quote bars to the span of those lines with
`partition_point` on `origin.y`. `bars` is built in line order, so it is sorted
by top; the field's doc says so. `NameField` shares the paint and stays correct:
its mask is its own bounds. A mask above or below the document paints at most the
padding lines.

The saving is the walk, not the atlas. gpui's `WrappedLine::paint` already skips
glyphs outside the mask before `paint_glyph` (`text_system/line.rs`,
`max_glyph_bounds.intersects(&content_mask.bounds)`), so an off-screen glyph never
reached the atlas or the rasterizer. What an idle frame paid was one
`WrappedLine::paint` call per line and one test per glyph, for every line of the note.

Tests: `a_frame_paints_only_the_lines_in_view` and
`a_frame_paints_the_bars_of_the_lines_in_view` read `painted()`, a test-only
record of the last paint's line and bar ranges. Four mutations were applied:
painting every line, using `mask.top()` without subtracting `origin.y`, painting
every bar, and dropping the `+ 1` on `end`. Each fails a test. The `+ 1` mutation
needed a lower bound the contract's assertions did not give: at the top of the
document, `lines.len() >= fit`, where `fit` is the viewport height in body rows
rounded up. At the end of the document `end` is `lines.len()` with or without the
`+ 1`, so `end == TALL + 1` alone does not catch it.

Measured (release probe, median of 3 runs, ms per idle draw):

| size | before | after | ratio |
|---|---|---|---|
| 5KB | 0.062 | 0.050 | 0.81x |
| 50KB | 0.162 | 0.048 | 0.30x |
| 500KB | 1.307 | 0.057 | 0.044x |

Acceptance: idle at 500 KB is 0.044x of before (limit 0.5x), and idle at 5 KB is
0.81x (limit 1.1x). An idle draw no longer grows with the note.

An edit still pays for its reshape; the keystroke series moved with the paint
(median of 3 runs, ms):

| size | series | before | after |
|---|---|---|---|
| 5KB | typed-warm-x | 0.247 | 0.228 |
| 5KB | typed-cold-x | 0.319 | 0.302 |
| 50KB | typed-warm-x | 0.931 | 0.819 |
| 50KB | typed-cold-x | 1.005 | 0.890 |
| 500KB | typed-warm-x | 13.018 | 11.438 |
| 500KB | typed-cold-x | 12.790 | 11.226 |

Shots: all 54 scenes are byte-identical before and after (`cmp`), among them
`reload-scroll` (scrolled), `markdown-blocks` (bars), `sidebar-naming` and
`sidebar-renaming` (`NameField`). `cargo mutants` does not cover `inkra-app`, so none ran.

## Item 3: `line_for_offset` by bisection

`DocumentLayout::line_for_offset` scanned `lines` from the first line.
`row_for_offset`, `position_for_offset` and the hit tests call it, several
times per frame. CLAUDE.md allows no per-frame scan of the whole document.
No timing is recorded: the rule decides.

Line ends ascend and `lines` is never empty, so `partition_point` finds the
first line whose end is not below the offset. An offset equal to a line's end
stays on that line. An offset past the last line clamps to the last line,
because a stale `NameField` layout can describe other text than the buffer.

### As built (S1)

`line_for_offset` is `partition_point(|line| line.range.end < offset)` clamped
to the last line. `an_offset_past_the_end_lands_on_the_last_row` pins both
line-end edges and the clamp. Two mutations fail it: `<` to `<=`, and removing the clamp.

## Item 4: UTF-16 conversions from byte 0

`Buffer::to_utf16` and `from_utf16` (`crates/text/src/buffer.rs`) scan the
text from byte 0. Audit bench, near the end of the text:

| lines | `to_utf16` | `from_utf16` |
|---|---|---|
| 1k | 15.3 us | 18.9 us |
| 10k | 154 us | 191.6 us |
| 100k | 1.57 ms | 1.93 ms |

The platform's input protocol calls these on marked-text and caret queries.
Kept: the cost is linear in the distance from byte 0, and the goal is a cost
proportional to the distance from a line start or one cached checkpoint.

## Item 5: `reveal::touched` scans twice

Dropped. `Iterator::eq` already walks both `touched` iterators in lockstep,
which is one pass. The conceal element ranges are not monotone, so a windowed
scan is unsound.

## Item 6: anchor slots never reused

Dropped. A tombstoned slot costs +0.7 us per edit at 3000 tombstones
(955 ns against 255 ns, `buffer_insert_after_parks`). The buffer is dropped
when its note closes. Reuse needs a generation in `Anchor`, which is a type
change and belongs to batch 3.

## Item 7: sidebar Strings per row

Dropped. `uniform_list` builds rows only for its visible range. That is about
40 rows, 3 Strings each, at about 33 ns per String (50k path Strings took
1.67 ms). The cost is about 4 us per frame and does not grow with the vault.

## Item 8: `Layout::new` allocations

Dropped. The two allocations per note take 1.67 ms of `NoteSet::build`'s
63.17 ms at 50k notes (2.6%). They run on the background `BuildJob`, not in a frame.

## Item 9: `margin_width` every frame

Audit claim: `margin_width` is recomputed every frame. The rule: below 50 us
per frame, drop. Measured over the idle draws at 50 KB, three runs:

| run | mean per call | calls |
|---|---|---|
| run 1 | 1.135 us | 40 |
| run 2 | 1.149 us | 40 |
| run 3 | 1.185 us | 40 |

Median 1.149 us. gpui's `shape_line` cache serves the six markers. Dropped.

## Item 10: `recent` hasher

`empty, 20 recent` takes 315 us on one core, and 50k SipHash lookups take
0.269 ms. The audit says FxHash. See the deviation below.

## Deviations from Zed

- Item 1: Zed shapes the visible rows every frame, which keeps gpui's cache
  warm. Inkra shapes the whole note, only on a reshape, so an idle frame keeps
  nothing warm. Following Zed needs Virtualization, which is deferred. Inkra
  instead carries each line's shape from the previous `DocumentLayout` when every
  `shape_text` argument is equal (S2), at the same index or at the start an
  edit moved the line to (S3).
- Item 2: Zed paints rows from the scroll position plus one. Inkra's element
  does not own its scroll (`docs/roadmap.md`, deferred), so it culls by gpui's
  content mask, because the div owns the scroll.
- Item 4: Zed's rope keeps UTF-16 lengths in chunk summaries. Inkra keeps a
  String plus one checkpoint, because a rope is a type change.
- Item 10: the audit says FxHash. Inkra uses foldhash, which is already a
  dependency and the local convention.

## Non-goals

- Batch 3 type changes, including a generation in `Anchor`.
- Batch 4 file splits: `buffer.rs` stays over 400 lines until batch 4.
- Virtualization stays deferred (`docs/roadmap.md`).

## Appendix: the probe

Apply it with `git apply`, run it, then `git checkout -- crates/app/src/shot crates/app/src/editor/element.rs`.
The `element.rs` hunk is for item 9 only.

```diff
diff --git a/crates/app/src/editor/element.rs b/crates/app/src/editor/element.rs
index 34da50c..037d90f 100644
--- a/crates/app/src/editor/element.rs
+++ b/crates/app/src/editor/element.rs
@@ -52,9 +52,12 @@ impl Element for TextElement {
         // Read here: the closure runs in `compute_layout`, outside the div's
         // text style, where `window.text_style()` would be the default instead.
         let text_style = window.text_style();
+        let probe_start = std::time::Instant::now();
+        let probe_margin = margin_width(&text_style, window);
+        eprintln!("margin_width {:.3}", probe_start.elapsed().as_secs_f64() * 1e6);
         let inset = Inset {
             column: column_width(&text_style, window),
-            margin: margin_width(&text_style, window),
+            margin: probe_margin,
         };
         // Wide enough to centre the column between two margins; D12 reserves only the left one.
         let widest = inset.column + inset.margin * 2.;
diff --git a/crates/app/src/shot/scenes.rs b/crates/app/src/shot/scenes.rs
index ee2481e..29957cd 100644
--- a/crates/app/src/shot/scenes.rs
+++ b/crates/app/src/shot/scenes.rs
@@ -85,6 +85,11 @@ const LINKED: &[(&str, &str)] = &[
 
 /// Not a function body: it grows with every scene, past clippy's `too_many_lines`.
 const CATALOGUE: &[Scene] = &[
+    Scene {
+        name: "probe-perf",
+        shows: "throwaway probe",
+        open: probe_perf,
+    },
     Scene {
         name: "empty-no-vault",
         shows: "launch with no argument: the empty state, and no editor at all",
@@ -514,3 +519,77 @@ fn backlinks(stage: &mut Stage) -> WindowHandle<AppShell> {
     stage.click_tree_row(window, 0.);
     window
 }
+
+fn probe_sample() -> String {
+    let paragraph = "word ".repeat(60);
+    format!(
+        "Plain prose opens the sample and runs for a little while before anything else.\n\
+         # A heading\n\
+         Setext heading\n\
+         ==============\n\
+         Some *emphasis*, **bold**, `code`, and a [[link]] in one line.\n\
+         > quote one\n\
+         > quote two\n\
+         > quote three\n\
+         - [ ] a task\n\
+         {paragraph}\n\n"
+    )
+}
+
+fn probe_perf(stage: &mut Stage) -> WindowHandle<AppShell> {
+    use std::time::Instant;
+    let sample = probe_sample();
+    let mut last = None;
+    for kb in [5, 50, 500] {
+        let text = sample.repeat(kb * 1024 / sample.len());
+        let window = stage.open_note(&text);
+        stage.focus(window);
+        stage.keys(window, "cmd-up");
+        let run = |stage: &mut Stage, idle_frames: usize, key: &str| -> f64 {
+            let mut total = 0.;
+            for _ in 0..20 {
+                for _ in 0..=idle_frames {
+                    stage.draw(window);
+                }
+                let t0 = Instant::now();
+                stage.keys(window, key);
+                stage.draw(window);
+                total += t0.elapsed().as_secs_f64() * 1e3;
+            }
+            total / 20.
+        };
+        let warm_x = run(stage, 0, "x");
+        let cold_x = run(stage, 2, "x");
+        let warm_enter = run(stage, 0, "enter");
+        let cold_enter = run(stage, 2, "enter");
+        // The key's settle draws once: no explicit draw after it, so 0 idle frames is truly warm.
+        let typed = |stage: &mut Stage, idle_frames: usize, key: &str| -> f64 {
+            let mut total = 0.;
+            for _ in 0..20 {
+                for _ in 0..idle_frames {
+                    stage.draw(window);
+                }
+                let t0 = Instant::now();
+                stage.keys(window, key);
+                total += t0.elapsed().as_secs_f64() * 1e3;
+            }
+            total / 20.
+        };
+        let typed_warm = typed(stage, 0, "x");
+        let typed_cold = typed(stage, 2, "x");
+        let mut idle = 0.;
+        eprintln!("idle-begin {kb}");
+        for _ in 0..20 {
+            let t0 = Instant::now();
+            stage.draw(window);
+            idle += t0.elapsed().as_secs_f64() * 1e3;
+        }
+        eprintln!("idle-end {kb}");
+        idle /= 20.;
+        println!(
+            "probe {kb}KB warm-x {warm_x:.3} cold-x {cold_x:.3} warm-enter {warm_enter:.3} cold-enter {cold_enter:.3} idle {idle:.3} typed-warm-x {typed_warm:.3} typed-cold-x {typed_cold:.3}"
+        );
+        last = Some(window);
+    }
+    last.unwrap()
+}
diff --git a/crates/app/src/shot/stage.rs b/crates/app/src/shot/stage.rs
index b35acc6..78cc004 100644
--- a/crates/app/src/shot/stage.rs
+++ b/crates/app/src/shot/stage.rs
@@ -293,6 +293,15 @@ impl Stage {
             .unwrap_or_else(|| panic!("quick open's row {ix} is drawn"))
     }
 
+    pub(crate) fn draw(&mut self, window: WindowHandle<AppShell>) {
+        self.cx
+            .update_window(window.into(), |_, window, cx| {
+                window.refresh();
+                window.draw(cx).clear(cx);
+            })
+            .unwrap_or_else(|error| panic!("draw the window: {error}"));
+    }
+
     /// Pixels are device pixels: a 1000x640 window yields 2000x1280 on a 2x display.
     pub(crate) fn capture(&mut self, window: WindowHandle<AppShell>) -> RgbaImage {
         self.settle();
```
