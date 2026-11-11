# Search and backlink rows as prose

Date: 2026-11-11. Status: **proposed**.

Written on the branch `backlinks-context-2b` (`e18da99a`). A throwaway spike,
branch `spike/plain-row-text` (commits `6b6bb977`, `ac170709`), measured every
figure below. The spike never merges.

## Why

Inkra shows one line of a note in three lists. Quick open (`⌘P`) shows it as
prose. Vault search (`⌘⇧F`) and the backlinks panel show the raw source bytes.

For the note line `- **Buy** milk for the *team*`:

- `⌘P` shows `Buy milk for the team`.
- `⌘⇧F` shows `- **Buy** milk for the *team*`. The spike's first test,
  `spike_context_is_raw_today`, failed on the base with exactly this left value.
- A backlink row shows the same raw bytes before its link.

The user decided the `⌘P` rule on 2026-09-27: "The second line shows prose, not
markdown syntax" ([`2026-09-25-smart-quick-open-design.md`](2026-09-25-smart-quick-open-design.md),
lines 510-511). The rule and its code exist. The other two lists do not use
them. This spec makes them use the same rule, the same way.

## Design

### P1. One rule: `inkra_markdown::markup`, unchanged

`markup(line, at_line_start)` (`crates/markdown/src/markup.rs:13-16`) gives the
bytes the plain form drops: leading block markers, and emphasis, strong and
code-span delimiters. Link, strikethrough and code-block syntax stays. The
three lists use this rule and no other.

Rejected: a second rule for rows, for example one that also drops wikilink
brackets. Two rules for "prose" would make `⌘P` and `⌘⇧F` disagree on one line.

### P2. The plain form is made in `index`, in `Hit::new` and `link_context`

`Hit::new` (`crates/index/src/search/hit.rs:19-30`) and `link_context`
(`crates/index/src/context.rs:19-31`) build the plain context themselves, as
`around_match` does for `⌘P` (`crates/index/src/find/excerpt.rs:64-90`). The
steps:

1. `window::around` cuts the window on raw bytes, as today.
2. `line_around` widens it by up to `LINE_BYTES` (1 KiB) each side, inside its
   line. This is the parsed context.
3. `Plain::new` drops the markup of the context and keeps the window's part.
4. `Plain::map` maps the hit, or the link, into the plain text.

The spike's helper `shown(text, window) -> (Plain, from)` does steps 2-3 for
all three callers. `from` skips the leading whitespace of a window that starts
its line, as `excerpt.rs:65-70` does.

Why `index` and not `app`: the parse needs the bytes outside the 160-byte
window. Only `index` holds the note's text. The case: a `**` opened 200 bytes
before the hit. The spike test `spike_app_side_sees_only_the_window` shows it.
On the raw window `"w w … bold hit** after"`, `markup` keeps the closing `**`.
The index side, with the widened context, shows `"w w … bold hit after"`.

This amends one sentence of
[`2026-09-29-row-text-design.md`](2026-09-29-row-text-design.md) (lines 76-77):
"The index gives the raw context". The one-line rule (`one_line`, control
chars) stays in `app`. Only the markup drop moves into the context, as `⌘P`'s
`Excerpt` already has it.

Rejected: the app-side design. See "Rejected alternatives".

### P3. `Plain` moves to the crate root of `inkra-index`

`crates/index/src/find/plain.rs` becomes `crates/index/src/plain.rs`, declared
in `crates/index/src/lib.rs`. `Plain`, its `text` field, `new`, `map` and
`LINE_BYTES` go from `pub(super)` to `pub(crate)`. `line_around` leaves
`excerpt.rs:93-104` and joins them, `pub(crate)`. `shown` joins them too, and
`around_match` calls it in place of its own copy of the same lines
(`excerpt.rs:65-77`).

Three users: `find` (`preview.rs:30`, `excerpt.rs:73`), `search/hit.rs` and
`context.rs`. The spike built and passed with this move.

Rejected: a second type that drops markup, or a trait. The repo rule is "reuse
`Plain`", and one implementation is no seam.

### P4. What stays raw

- `Hit.range` and `Hit.line` stay source positions. The app opens the note at
  them. The spike prints them unchanged: `milk` in `- **Buy** milk …` keeps
  `range` `10..14` and line 1; a hit in a fenced line keeps line 2.
- The link ranges the index stores (`SourceLink`, the occurrences
  `slot.rs:62-65` reads) stay source positions.
- `LinkContext` is not persisted (`grep -rln LinkContext crates` lists no
  `persist` file), so no cache format changes.

### P5. The window: cut on raw bytes, then dropped

The window stays 60 bytes before the hit and 100 after, inside its line
(`crates/index/src/window.rs:5-19`), cut on raw bytes. The markup is dropped
after the cut. A row therefore shows fewer than 160 bytes when the window held
markup.

Rejected: drop first, then cut 160 plain bytes. It needs the plain form of the
whole widened context before the cut, then a second mapping back to source
offsets for the cut's edges. `⌘P`'s text rows already cut on raw bytes
(`2026-09-25-smart-quick-open-design.md`, line 548: "A text row's window stays
as it was"). Two lists cutting two ways would differ on one line for no
visible gain: the row ellipsizes at its width anyway.

### P6. A match inside markup

A hit on a word that `markup` drops maps to an empty `context_match`. The spike
(`spike_print_cases`) shows two cases:

- `1. first`, query `1`: context `"first"`, match `0..0`.
- `- [x] done item`, query `x`: context `"done item"`, match `0..0`.

The row shows the line without the marker and lights nothing.
`LitText::line` drops the empty range (`crates/app/src/row_text.rs:29`).
`LitText::context` cuts at the smallest of the indent and the range start
(`row_text.rs:49-54`); both are 0 here, so it cuts nothing. The hit still
counts in its note's header and still opens at its source range.

`⌘P` does the same: "A word of the markup, like an ordered item's number, is
not shown" (`excerpt.rs:82-85`).

Rejected: drop such a hit from the list. The header count and the list would
then disagree, and `Enter` on that line would no longer reach the marker.
Rejected too: keep the raw form for that one row. It is a special case, and
two forms in one list read as a bug.

### P7. A link in a backlink row keeps its syntax

`link_context` still requires the link to start with `[` or `![`
(`context.rs:23`). `markup` keeps link syntax (`markup.rs:14`), so the mapped
`link` covers the whole link. The spike test
`spike_link_covers_the_whole_link` asserts it for eleven lines: a link after
`**bold**`, inside `*emphasis*`, in a list item, in a quote, in an ordered item
with an alias, in a task item, in a heading, after a code span (an embed), as
the whole of `**…**` and of `*…*`, and a markdown link `[md](Target.md)`. A
twelfth case puts the link 50 bytes after an opening `**` that closes after it.

The panel's `split_at_first` (`crates/app/src/backlinks/mod.rs:205`) splits the
plain text at the mapped link, unchanged.

### P8. Doc comments that become false

- `crates/index/src/search/hit.rs:13`: "Part of the hit's line, for display
  only; it always holds the whole hit." The context is now the plain form, and
  a hit inside markup is not in it (P6). New text: "The plain form of part of
  the hit's line, for display only."
- `hit.rs:15`: "The hit inside `context`." New text: "The hit inside
  `context`; empty when the hit is all markup."
- `crates/index/src/context.rs:1` and `:9`: "A link's window of its line …".
  New text says the window's plain form, and that the link keeps its syntax.
- `crates/app/src/row_text.rs:45`, `LitText::context`: "after the leading
  whitespace". It stays true. The index now skips the indent of a window that
  starts its line, so the trim matters only for a mid-line window that starts
  on a space. No change.

### P9. The accessibility label follows

The picker row's `label` is `format!("line {line}: {}", context.text())`
(`crates/app/src/vault_search/rows.rs:113-128`). It reads the plain form with
no change in `app`.

## What the spike showed

- **Defect.** `spike_context_is_raw_today` failed on the base and passed under
  the spike.
- **Lines inside code blocks.** `markup` parses one line and knows no block.
  A fenced line `let v = a*b*c;` shows `let v = abc;`. A fenced line
  `**kwargs and *args*` shows `**kwargs and args`. A line `a = *p * *q;` stays
  whole: its `*` form no emphasis pair. An indented code line loses its indent,
  like any line. `⌘P`'s `around_match` runs the same `Plain::new` over the same
  widened context (`excerpt.rs:71-77`), so it has the same behaviour today.
- **Not this design's defect.** `the _team_ wins` with query `team`, and
  `snake_case_word` with query `case`, give no hit at all. The search's words
  keep `_` inside them (`locate`, `search/run.rs:37-56`, which the spike does
  not touch). It is out of scope here.
- **Look.** The spike added `1. **Buy** milk for the *plan* and `` `plan` `` it.`
  to `projects/inkra/plan.md` in the `vault-search-results` scene. The picture
  shows line 1 as `Plan` (the `#` dropped), and line 4 twice as
  `Buy milk for the plan and plan it.`, each row lighting its own `plan`. Line
  numbers stay 1 and 4. In the `backlinks` scene, `- **Books** to read, listed
  from the [[index]].` shows `…ks to read, listed from the [[index]].`, and
  `> Back to *[[index]]* after lunch.` shows `Back to [[index]] after lunch.`,
  the link in the link colour. The leading `…` is the panel's start ellipsis
  (`crates/app/src/backlinks/render.rs:48`), not the drop.

## Cost

One search lists at most 1,000 hits (`const LIMIT: usize = 1_000;`,
`crates/app/src/vault_search/mod.rs:36`), at most 100 per note
(`HITS_PER_NOTE`, `crates/index/src/search/run.rs:11`). Each hit now costs one
`markup` pass over up to about 2 KiB plus the window.

### The plan's bench: too noisy to decide

`cargo bench -p inkra-index --bench read_search -- --quick`, vault
`$TMPDIR/inkra-bench-50000`, term `penrara`, 988 notes, 1,000 hits. Runs in
order, each a separate command:

| Run | Branch | `resident` | `read4` |
|-----|--------|-----------:|--------:|
| 1 | base `e18da99a` | 7.94 ms | 15.39 ms |
| 2 | spike | 9.62 ms | 19.39 ms |
| 3 | base `e18da99a` | 9.71 ms | 22.29 ms |
| 4 | spike | 9.30 ms | 20.53 ms |

The base moved 22 % between runs 1 and 3 with no code change. Criterion gave
run 2 as +18 % and +25 % at p = 0.08. A full run of
`cargo bench -p inkra-index --bench search -- "search/50k"` under the spike
moved the zero-hit control case (`"projetzz"`, which builds no `Hit`) by
+25 %. The machine was noisy; these runs measure the machine, not the change.

### Measured in one process

The spike's `spike_time_hit_new` alternates the old construction (`around`,
copy, offset) and the new `Hit::new`, 41 rounds each, and reports medians.
Command, on the spike branch at `ac170709`:
`cargo test --release -p inkra-index --lib spike_time_hit_new -- --ignored --nocapture`.

| Case, 1,000 hits | Before (median) | After (median) | Extra per hit |
|------------------|----------------:|---------------:|--------------:|
| Bench vault `penrara`, lines mean 227 B, max 640 B | 48 µs / 60 µs | 555 µs / 677 µs | 0.51 µs / 0.62 µs |
| A 50-byte list line with strong, emphasis, code | 35 µs | 589 µs | 0.55 µs |
| A 4,136-byte line of prose, no markup | 30 µs | 861 µs | 0.83 µs |
| A 4,410-byte line, emphasis every 10 bytes | 53 µs / 85 µs | 15.6 ms / 23.6 ms | 15.5 µs / 23.5 µs |

Two figures are two runs. On the bench vault, 0.51-0.62 ms per search is
5-8 % of `resident` (7.9-9.7 ms above) and 3-4 % of `read4`. That is under the
plan's 10 % gate.

The last row is the risk. A long line dense with emphasis costs about 20 µs
per hit, so a search whose 1,000 hits all sit on such lines gains about
16-24 ms. This probably stays rare: it needs paragraph-long lines with markup
every few words, and a term on most of them. `⌘P` pays the same per row, but
for at most 100 rows. S1 adds this case to the `search` bench, so the build
measures it. A cheaper form, if it is needed, is in "Deferred".

## Tests

**Tests that fail under the spike: none.** Before the spike:
`cargo test -p inkra-index` passed 573, `cargo test -p inkra-app --lib
vault_search` 38, `cargo test -p inkra-app --lib backlinks` 48. Under the spike
the same commands passed 574 (the new spike test), 38 and 48, and
`cargo test --workspace` reported no failure. No existing test asserts raw
markers in a context:

- `context_keeps_60_bytes_before_and_100_after_within_the_line`
  (`crates/index/src/search/tests.rs:250-262`) uses plain words.
- The model oracle calls `Hit::new` itself (`search/model_tests.rs:69`), so it
  agrees with any `Hit::new`. It proves nothing about the plain form.
- `the_window_holds_the_link_and_its_line` and
  `a_long_link_is_clamped_to_the_window` (`crates/index/src/context_tests.rs`)
  hold no markup.
- The app's `listed` (`crates/app/src/vault_search/tests.rs:61`) fixtures and
  `an_indented_link_drops_its_indent_and_draws_a_tab_as_a_space`
  (`crates/app/src/backlinks/tests.rs:356`) hold no markup either.

**New tests.** Each asserts literal strings, the shown text and the lit text,
never a value computed by `Hit::new` or `link_context`.

S1, in `crates/index/src/search/tests.rs`, through `context_of`:

- `a_context_drops_markup`: `- **Buy** milk for the *team*`, `milk` → shown
  `Buy milk for the team`, lit `milk`.
- `markup_opened_before_the_window_still_drops`: `**` then 200 bytes then
  `bold hit**` → the shown text ends `bold hit`, lit `hit`.
- `a_hit_inside_markup_lights_nothing`: `1. first`, `1` → shown `first`, match
  empty; and `- [x] done item`, `x` → shown `done item`, match empty.
- `a_hit_keeps_its_source_range_and_line`: a fenced block, hit on its second
  line → `range` and `line` are the source's.
- `a_line_in_a_code_block_drops_like_any_line`: fenced `let v = a*b*c;`, `let`
  → shown `let v = abc;`. It pins the accepted behaviour (see "Deferred").

S1, in `crates/app/src/vault_search/tests.rs`: one keystroke test whose note
holds `1. **Buy** milk for the *plan*`; `listed` prints `1: Buy milk for the
plan`.

S2, in `crates/index/src/context_tests.rs`:

- `a_link_keeps_its_syntax_and_the_markup_around_it_drops`: the eleven lines of
  P7, each with its literal shown text and literal lit link.
- `markup_open_before_a_link_drops`: the twelfth case of P7.

S2, in `crates/app/src/backlinks/tests.rs`: a source line
`> Back to *[[index]]* after lunch.`; the row reads `Back to ` before the link
and `[[index]] after lunch.` from it.

## Slices

Vertical. Each slice ends green on `./scripts/check.sh` and runs the mutants
command on its diff:
`git diff main... > $TMPDIR/m.diff && cargo mutants -j 4 --in-diff $TMPDIR/m.diff`.

- **S1. `⌘⇧F` rows.** Move `Plain` and `line_around` (P3), add `shown`, make
  `around_match` use it. `Hit::new` builds the plain context (P2). Fix the doc
  comments of `hit.rs` (P8). The index and app tests of S1. A
  `vault-search-results` scene line with markup, rendered and looked at. A
  `search` bench case of one long markup-dense line.
- **S2. Backlink rows.** `link_context` builds the plain context. Fix the doc
  comments of `context.rs` (P8). The index and app tests of S2. A `backlinks`
  scene line with markup, rendered and looked at.

S2 depends on S1's move. Each slice is one PR.

## Deferred

- **Code-block lines.** A hit in a fenced or indented code line drops `*`
  pairs and backticks like prose. The index knows no block at a hit. `⌘P` has
  the same behaviour. Trigger: a user reports a code line read wrong in a row.
- **A per-line parse cache.** Up to 100 hits of one note can sit on one long
  line, and each reparses the same ~2 KiB. One parse per line per note would
  bound the dense case of "Cost". Trigger: the S1 bench case, or a measured
  search, slows by more than 10 %.
- **A smaller parse radius for search.** `LINE_BYTES` is `⌘P`'s 1 KiB each
  side. A search-only radius would cut the dense case but drop fewer far
  delimiters. Same trigger as the cache.
- **Wikilinks as their alias.** See the first open question.

## Open questions

1. **Should a backlink row show a wikilink as its alias or target, without the
   brackets?** That changes `markup` or adds a second rule (P1). Recommended:
   no, keep the syntax in this design. The link is the lit part, and the
   brackets tell which link it is.
2. **Should a line inside a fenced code block keep its bytes?** The index does
   not know blocks at a hit. Recommended: accept the `⌘P` behaviour, as listed
   under "Deferred".
3. **Should the picker's accessibility label (`label` in
   `vault_search/rows.rs`) read the plain form too?** Recommended: yes. It
   follows from the context with no code (P9).
4. **Should a hit that is all markup (`1`, `x` in a task box) be listed?**
   Recommended: yes, unlit, as `⌘P` does (P6). The count stays true and the hit
   still opens at its source.
5. **Is the dense-markup cost acceptable without the per-line cache?**
   Recommended: yes for S1, gated by the new bench case. The bench vault pays
   0.5-0.6 µs per hit; the dense case is a constructed worst case.

## Rejected alternatives

**The app-side design.** `app` would call `inkra_markdown::markup` on the
context in `HitRow::new` (`vault_search/rows.rs:113-128`) and in `occurrences`
(`backlinks/mod.rs:199-207`).

- It sees only the 160-byte window, no bytes outside it. A `**` opened 200
  bytes before the hit keeps its closing `**` in the row
  (`spike_app_side_sees_only_the_window`). The same line then reads one way in
  `⌘P` and another in `⌘⇧F`.
- It needs its own mapping type for the lit range, a second `Plain`. The repo
  rule is "reuse `Plain`"; `Plain` lives in `index`, and `app` cannot reach a
  `pub(crate)` type.
- It would make `app` depend on `inkra-markdown` for a text rule. `app`
  depends on it already, for the editor's styling, so this point is weak.

What it would win: it could drop markup only for the rows drawn, about a
screen of rows instead of 1,000 hits. That is the fallback if the dense case of
"Cost" proves real. It still needs the widened context from `index`, so it
would move the parse, not remove the index's part.

**Drop first, then cut.** See P5.

## Deviations from Zed

Zed's Text Finder shows the source line unchanged. `render_matched_line`
(`vendor/zed-reference/crates/search/src/text_finder/delegate.rs:1342-1427`)
takes a bounded window around the match, trims the indent when the window
starts its line, and adds syntax highlights over the source text. Its files
are code, so the source is what a user reads. Inkra's notes are prose, so the
row drops the markup, as the user decided for `⌘P`. The window's indent trim
matches Zed's.
