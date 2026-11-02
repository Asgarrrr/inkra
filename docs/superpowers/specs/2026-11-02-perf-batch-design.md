# Performance batch 2: measure, then change

Status: **in progress 2026-11-02.** S1 records the baseline and lands item 3.
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
- The sample is plain prose, a `#` heading, a setext heading, emphasis, bold,
  code, a wikilink, a three-line quote, a task and a 300-char wrapping paragraph.
- Items 4, 5, 6, 7, 8 and 10 take the audit's `cargo bench` and counted-test
  figures. S1 did not re-run them; the slice that changes one re-runs its
  bench first and records before and after.
- Bench commands: `cargo bench -p inkra-text -- --quick`, `cargo bench -p inkra-markdown -- --quick`.

## Decisions

| item | measured cost | keep/drop | slice |
|---|---|---|---|
| 1 cold reshape after an idle redraw | cold-x 2.719 ms vs warm-x 2.744 ms at 50 KB (0.99x) | drop | cancelled (S2, S3) |
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
Dropped. The shaped lines live in `DocumentLayout`, and an edit reshapes only
the lines it touches, so gpui's cache is not on the path. S2 and S3 are cancelled.

The 16 ms an edit costs at 500 KB is not shaping from a cold cache: warm and
cold agree. Its cause is a separate question, outside this batch.

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
  warm. Inkra carries whole-note shapes across layouts until Virtualization.
  The probe shows the cache is not on Inkra's path, so no change follows.
- Item 2: Zed culls by scroll position plus one row. Inkra culls by gpui's
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
index ee2481e..c702557 100644
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
@@ -514,3 +519,62 @@ fn backlinks(stage: &mut Stage) -> WindowHandle<AppShell> {
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
+            "probe {kb}KB warm-x {warm_x:.3} cold-x {cold_x:.3} warm-enter {warm_enter:.3} cold-enter {cold_enter:.3} idle {idle:.3}"
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
