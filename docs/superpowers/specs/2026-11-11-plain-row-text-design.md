# Search and backlink rows as prose

Date: 2026-11-11. Status: **proposed**.

Written on the branch `backlinks-context-2b` (`e18da99a`). A throwaway spike,
branch `spike/plain-row-text` (commits `6b6bb977`, `ac170709`, `ef02c02a`),
measured every figure below. The spike never merges.

Revision 1 (same day) bounds the parse cost by construction (P10) and keeps
the bytes of a code-block line (P11, P12). It replaces the first "Cost"
section, which deferred both.

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
2. `line_around` widens it by up to `LINE_BYTES` each side, inside its line.
   This is the parsed context. `LINE_BYTES` becomes 256 (P10).
3. The context's markup is parsed once, and the window's part is kept (P10).
   A window on a code-block line keeps its bytes instead (P11).
4. `Plain::map` maps the hit, or the link, into the plain text.

The spike's helper `shown_all(text, windows, code) -> Vec<(Plain, from)>`
does steps 2-3 for a note's hits, ascending. `shown(text, window)` is its
one-window form, for `⌘P`. `from` skips the leading whitespace of a window
that starts its line, as `excerpt.rs:65-70` does.

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
`excerpt.rs:93-104` and joins them, `pub(crate)`. `shown` and `shown_all`
join them too, and `around_match` calls `shown` in place of its own copy of
the same lines (`excerpt.rs:65-77`). A private `Parsed` holds one context's
dropped ranges; `Plain::new` becomes `Parsed::new(..).plain(..)`.

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
  `persist` file). The cache format changes for P11 only.

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

### P10. One parse per line per note, 256 bytes each side

`shown_all` takes a note's windows in ascending order. It joins consecutive
windows whose contexts overlap into one cluster, parses the cluster's union
once, and cuts each window's piece from that one parse. Contexts never cross a
`\n` (`line_around`), so a cluster lies on one line. Two clusters' contexts are
disjoint, so each byte of a line parses at most once per note.

The bound on one parse:

- One context is at most 160 + 2 × `LINE_BYTES` = 672 bytes.
- A cluster of k windows is at most k × 672 bytes, and k ≤ `HITS_PER_NOTE`
  (100). One parse, and all of one note's parses together, read at most
  67,200 bytes.
- `⌘P` parses one window per row: at most 672 bytes.
- The backlinks panel calls `link_contexts(text, occurrences)`, built on
  `shown_all`, in place of one `link_context` per occurrence (`slot.rs:62-65`).
  Occurrences are in document order (`slot.rs:12`), so two links on one line
  share one parse there too.

`LINE_BYTES` goes from 1 KiB to 256 bytes, for all three lists: one rule, one
`Plain`. It also caps the first line of `⌘P`'s preview (`preview.rs:89`). That
line becomes 256 bytes, which is probably still longer than a row shows.

What 256 bytes misses: a delimiter whose partner lies more than 256 bytes
outside the window. For a `**` opened before the hit, that is more than about
316 bytes before it (60 in the window, 256 past it). The spike test
`a_delimiter_past_the_radius_stays` shows both sides: `**`, 100 bytes, then
`bold hit** after` drops both `**`; with 400 bytes in place of 100, the row
keeps `hit** after`. The case P2 was built for, a `**` 200 bytes before the
hit, still drops.

Why the cluster is the cache: the windows arrive in ascending order, so the
previous parse is the only one a window can reuse. It needs no key, no map
and no eviction.

Rejected:

- One parse per hit, the first spike: 1.73 ms against 77 µs for 100 hits on
  one line (see "Cost").
- A map from line to parse: it buys nothing over the cluster, because windows
  arrive in order, and it allocates per note.
- Parse the window alone (`LINE_BYTES` 0): it is the floor of "Cost", but it
  misses the 200-byte case of P2 and reads like the rejected app-side design.
- Keep 1 KiB: case (ii) of "Cost" costs three times more, and the prose case
  (v) passes 10 %.

### P11. A code-block line keeps its bytes

A window whose hit lies in a code block shows its bytes as written. `markup`
does not run on it.

**One definition of a code block.** `inkra_markdown` gains
`code_text(text) -> Vec<Range<ByteOffset>>`: the content of every fenced and
indented block, from pulldown-cmark's `Tag::CodeBlock` events with
`cmark::options()`. This is the parser and the options that `parse` uses for
`Markdown.code_blocks` (code-blocks spec, D1) and that `links` uses. Fences,
`> ` prefixes, a list item's indent and an indented block's four spaces are
not in the content. Contiguous lines merge into one range. The spike test
`code_text_is_each_block_without_fences_or_prefixes` pins nine cases: a top
fence, an indented block, a fence in a quote, a fence in a list item, an
unclosed fence, a `~~~` fence, a lazy paragraph line that is not code, a tab
indent, and an empty block.

**Computed when the index parses the note.** `links_and_code(text)` gives
`links` and `code_text` from one pass of the parser; both share one event
visitor (`code::visit`). `Source::parse` (`source.rs:30`) calls it, and
`Source` gains `code: Arc<[Range<ByteOffset>]>`.

**The cache.** The file holds the `Source`s (persisted-index spec, P2), so it
holds the code ranges too. `FORMAT_VERSION` goes from 3 to 4
(`persist/header.rs:10`, P3: "bumped by hand on any layout change"). The
derivation fingerprint hashes what the encoder writes, so the code ranges join
it with no second list, and its golden constant moves once. Every older cache
is rejected once; Inkra has no users yet, so no migration.

**The read.** `Candidate` gains `code`, filled from the note's `Source` by
`IndexSnapshot::candidate` (`query.rs:29-37`); `insert` writes the source and
the read together (`index/backlink.rs:16`). One function,
`Candidate::code_of(&self, text, stamp)`, gives the code ranges of a read:

- the candidate's ranges when the read's stamp is the candidate's stamp;
- else `code_text(text)`, one parse of that one note.

The second case is the text read differing from the text indexed. Search
still searches such a text and marks it stale (`search/text.rs:96-101`).
The stored ranges then point into another text. `Plain::raw` slices at them,
and an offset off a char boundary would panic, so they are never used on it.

**The callers.**

- `⌘⇧F`: `TextSearch::read` passes `candidate.code_of(&text, stamp)` to
  `note_hits` (`text.rs:96-101`).
- `⌘P`: `excerpt` gains a `code` argument. Its caller holds the candidate and
  the read (`crates/app/src/quick_open/list.rs:133-148`).
- Backlinks: none. A link is never in a code block (`links` drops them;
  `crates/markdown/src/links/tests.rs:115-120`), so a link's line is never a
  code line.

**At the hit.** `shown_all` looks up the window's last byte in the ranges
(`partition_point`). That byte is on the hit's line, past any `> ` prefix; the
window's first byte may sit in the prefix. The piece is shown from the later of
the window's indent end and the range's start, so the prefix and the block
indent drop as everywhere. A fence line itself is not code content: it goes
through `markup`, which keeps code-block syntax (P1).

The spike built candidate (a) below for correctness, through `note_hits`, and
measured (b)'s pass. It did not build (b)'s `Source` field, cache encoding or
`code_of`.

Rejected:

- **(a) `code_text` once per note with a hit, at search time.** It adds a full
  parse of every note with a hit: +5.8 ms on the bench vault, about 86 % of the
  search's own work (see "Cost").
- **(c) Something cheaper.** A prefilter on the text (` ``` `, `~~~`, a line
  starting with four spaces or a tab) is fence detection by hand, and the
  brief excludes it. Reusing `parse`'s `code_blocks` needs the full styling
  parse at index time, a heavier pass than `links`, and its ranges include
  fences and prefixes.
- **Reuse the stored ranges on a stale read.** See the panic hazard above.

### P12. A code span shows its content

An inline code span shows its content without the backticks: `markup` drops a
code span's delimiters (`markup.rs:13-16`), as `⌘P` does today. The spike test
`a_code_span_shows_its_content_without_backticks` pins it: ``see `code hit` now``
shows `see code hit now` and lights `hit`.

## What the spike showed

- **Defect.** `spike_context_is_raw_today` failed on the base and passed under
  the spike.
- **Lines inside code blocks.** `markup` parses one line and knows no block.
  A fenced line `let v = a*b*c;` shows `let v = abc;`. A fenced line
  `**kwargs and *args*` shows `**kwargs and args`. A line `a = *p * *q;` stays
  whole: its `*` form no emphasis pair. An indented code line loses its indent,
  like any line. `⌘P`'s `around_match` runs the same `Plain::new` over the same
  widened context (`excerpt.rs:71-77`), so it has the same behaviour today.
  P11 fixes it in the three lists. Under `ef02c02a`, the spike test
  `a_code_block_line_shows_its_bytes` passes for a top fence, a fence of
  `**kwargs and *args*`, an indented block, a fence in a quote and a fence in a
  list item; `a_lazy_line_after_a_paragraph_is_not_code` passes too.
- **Not this design's defect.** `the _team_ wins` with query `team`, and
  `snake_case_word` with query `case`, give no hit at all. The search's words
  keep `_` inside them (`locate`, `search/run.rs:37-56`, which the spike does
  not touch). It is out of scope here and gets its own plan: `is_word`
  (`crates/index/src/words/fold.rs:30-36`).
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
(`HITS_PER_NOTE`, `crates/index/src/search/run.rs:11`). Under P10, a note's
hits cost at most one `markup` pass over each line byte, and at most 67,200
bytes in all.

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

The spike's `spike_timing` (`crates/index/src/search/spike_timing.rs`, commit
`ef02c02a`) runs every variant in one process, interleaved, 41 rounds, and
reports medians. Command:
`cargo test --release -p inkra-index --lib spike_timing -- --ignored --nocapture`.
`LINE_BYTES` was set to 1 KiB, 256 and 0 for three runs.

Variants:

- **locate**: the search's scan for hits, `locate` (`search/run.rs`).
- **raw**: today's `Hit::new`: `around`, a copy, an offset.
- **first spike**: one `Plain::new` per hit, at 1 KiB (`ac170709`).
- **revised**: `shown_all` (P10), at the run's `LINE_BYTES`.

"Base" is locate + raw of the same run: the search's own work in this
process. The plan's `resident` measures more (the reads too) and was 7.9-9.7 ms
for case (iii) above. "Added" is a variant minus raw.

Cases:

- (i) 100 hits on one dense 4.4 KB line: `"hit "` then `"word *em* "` × 4,
  repeated 100 times. One note.
- (ii) 1,000 hits on 1,000 distinct dense 4.4 KB lines: 10 notes of 100 lines,
  each line 2.2 KB of `"word *em* "`, `hit`, 2.2 KB more.
- (iii) The bench vault `$TMPDIR/inkra-bench-50000`, term `penrara`: the
  first 987 notes that hold it, 1,000 hits, 3.5 MB.
- (iv) 100 hits 2.2 KB apart on one 220 KB dense line: a cluster's widest
  parse.
- (v) 1,000 notes, each one 4.4 KB prose line with one hit, no markup.

**(i), (ii), (iii): raw, first spike, revised.**

| Case | Base | Raw | First spike (1 KiB) | Revised, 1 KiB | Revised, 256 B |
|------|-----:|----:|--------------------:|---------------:|---------------:|
| (i) one dense line, 100 hits | 18 µs | 4.6 µs | 1.733 ms | 77 µs | 59 µs |
| (ii) 1,000 dense lines | 5.85 ms | 47 µs | 17.84 ms | 17.71 ms | 5.94 ms |
| (iii) bench vault | 6.70 ms | 63 µs | 640 µs | 617 µs | 608 µs |

**Added cost against base, every case.**

| Case | First spike (1 KiB) | Revised, 1 KiB | Revised, 256 B | Window only (0 B) |
|------|--------------------:|---------------:|---------------:|------------------:|
| (i) | +1.73 ms | +72 µs | +56 µs | +52 µs |
| (ii) | +17.8 ms (304 %) | +17.7 ms (302 %) | +5.89 ms (101 %) | +1.71 ms (29 %) |
| (iii) | +0.58 ms (8.6 %) | +0.55 ms (8.3 %) | +0.55 ms (8.2 %) | +0.45 ms (6.4 %) |
| (iv) | +1.70 ms | +1.83 ms | +0.57 ms | +0.17 ms |
| (v) | +0.87 ms (16.8 %) | +0.86 ms (16.6 %) | +0.43 ms (8.2 %) | +0.30 ms (5.5 %) |

Each run measured its own base; the percentages use it. A percentage for (i)
and (iv) is left out: one note, a base of 14-18 µs and 0.28-0.30 ms.

What the figures say:

- **The cluster fixes (i).** 100 hits on one line parse that line once:
  1.73 ms becomes 72 µs.
- **(ii) passes 10 % at every radius.** Its 1,000 lines are distinct, so the
  cluster has nothing to share. Even the window alone adds 29 %.
- **The floor of (ii), and its reason.** Window-only parses 1,000 × 160 B =
  160 KB in 1.71 ms, about 94 MB/s, or 1.7 µs per window. At 256 bytes it
  parses up to 672 KB in 5.89 ms, about 114 MB/s. The prose of (v) at 256 bytes
  parses the same 672 KB in 0.43 ms, about 1.5 GB/s. A line with an emphasis
  pair every 10 bytes costs pulldown-cmark about 13 times more per byte than
  prose. Bytes parsed × that speed is the floor; only fewer bytes lower it.
- **The candidate that lowers it: 256 bytes.** It cuts (ii) from +17.7 ms to
  +5.89 ms and (iv) from +1.83 ms to +0.57 ms. It brings the prose case (v)
  under 10 %. (iii) does not move: the bench vault's lines are short (mean
  227 B, max 640 B), so its cost is the fixed ~0.5 µs per hit. The delimiters
  it misses are shown by `a_delimiter_past_the_radius_stays` (P10).
- **Recommended: the cluster with 256 bytes.** The floor (0 bytes) saves
  another 4.2 ms on (ii) but drops P2's own case. (ii) is a constructed worst
  case: a thousand paragraph-long lines with markup every two words.

**On which thread.** `⌘⇧F` builds its hits off the foreground. The search
task is spawned on the background executor
(`crates/app/src/picker/runs.rs:168`), which calls `D::search` (`:173`). That
is the delegate's `search` (`crates/app/src/vault_search/mod.rs:149`), which
calls `TextSearch::read` (`:162`), which calls `note_hits`
(`crates/index/src/search/text.rs:101`), where the hits are built. The (ii)
cost therefore delays the answer by about 6 ms; it never holds a frame.

The backlinks panel builds its contexts on the foreground: its read job is a
`cx.spawn` (`crates/app/src/backlinks/job.rs:76`) whose batches land in
`this.update` (`:79`), through `land_batch` and `Slot::land` (`:100`) to
`link_context` (`crates/index/src/slot.rs:62-65`). `slot()`
(`crates/app/src/backlinks/mod.rs:189`) does the same from the text cache. At
the (ii) rate, a dense source line costs about 6 µs per occurrence there, and
P10 parses each line of a source once.

Where `⌘P`'s `excerpt` runs (the `stream_with` closure,
`crates/app/src/quick_open/list.rs:146-150`) was not checked in this
revision. It parses at most one 672-byte context per row.

**Code ranges (P11).** The same test times the index pass and (a):

| Case | `links` (today) | `links_and_code` (b) | `code_text` alone (a) | Revised + (a) |
|------|----------------:|---------------------:|----------------------:|--------------:|
| (iii) bench vault, 987 notes | 6.04 ms | 6.10 ms | 5.77 ms | 6.51 ms |
| (ii) 1,000 dense lines | 33.35 ms | 33.01 ms | 33.6 ms | 51.6 ms |

All at 1 KiB. (b)'s extra pass is within the noise: 6.04 ms against 6.10 ms,
and 33.35 ms against 33.01 ms. (a) adds 5.8 ms to the bench vault's search,
about 86 % of its base. The lookup at the hit is the gap between "Revised +
(a)" and "`code_text` alone" minus "Revised": about 0.1 ms, near the noise.

Over the whole bench vault, `spike_code_counts`, 7 rounds, median:

- 50,000 notes, 150 MB; 19,995 notes hold code; 27,317 ranges; at most 8 in
  one note.
- `links`: 264.0 ms. `links_and_code`: 267.3 ms, +1.3 %.

The memory of the field is an estimate from these counts, not a measurement:
16 bytes per `Source` for the slice pointer (0.8 MB), 16 bytes per range
(0.44 MB), and one `Arc` header per note with code (0.32 MB), about 1.6 MB in
all. The cache file grows by the encoded ranges. S1 measures both.

## Tests

**Tests that fail under the spike: none.** Before the spike:
`cargo test -p inkra-index` passed 573, `cargo test -p inkra-app --lib
vault_search` 38, `cargo test -p inkra-app --lib backlinks` 48. Under
`ac170709` the same commands passed 574, 38 and 48. Under `ef02c02a`, at
256 bytes, `cargo test -p inkra-index -p inkra-markdown` passed 723 with one
ignored (the timing), and `cargo test --workspace` passed 2,867 with one
ignored. No existing test asserts raw markers in a context:

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
never a value computed by `Hit::new`, `link_context` or `code_text`. The ones
marked "spike" exist and pass under `ef02c02a`.

S1, in `crates/markdown`:

- `code_text_is_each_block_without_fences_or_prefixes` (spike): the nine
  cases of P11.
- `links_and_code_is_links_and_code_text`: a proptest that one pass gives what
  the two functions give apart.

S1, in `crates/index`:

- `a_source_holds_its_code_ranges`: `Source::parse` on a fenced and an
  indented block.
- The cache round trip carries the code ranges; the header's golden test pins
  `FORMAT_VERSION` 4 and the moved derivation fingerprint.
- `code_of_reads_the_stored_ranges_at_the_stamp`, and
  `code_of_parses_a_text_read_at_another_stamp`: the second gives the read
  text's ranges, where the stored ones would be wrong.
- `a_code_line_excerpt_keeps_its_bytes`: `⌘P`'s `around_match` on a fenced
  `let v = a*b*c;` shows it as written.
- `a_delimiter_past_the_radius_stays` (spike): P10's two lines.

S2, in `crates/index/src/search/tests.rs`, through `context_of`:

- `a_context_drops_markup`: `- **Buy** milk for the *team*`, `milk` → shown
  `Buy milk for the team`, lit `milk`.
- `markup_opened_before_the_window_still_drops`: `**` then 200 bytes then
  `bold hit**` → the shown text ends `bold hit`, lit `hit`.
- `a_hit_inside_markup_lights_nothing`: `1. first`, `1` → shown `first`, match
  empty; and `- [x] done item`, `x` → shown `done item`, match empty.
- `a_hit_keeps_its_source_range_and_line`: a fenced block, hit on its second
  line → `range` and `line` are the source's.
- `a_code_block_line_shows_its_bytes` (spike): five lines, P11. It replaces the
  first spec's `a_line_in_a_code_block_drops_like_any_line`.
- `a_lazy_line_after_a_paragraph_is_not_code` (spike): `para` then
  `    not *code* hit` → `not code hit`.
- `a_code_span_shows_its_content_without_backticks` (spike): P12.
- `hits_on_one_line_share_its_parse`: two hits 100 bytes apart on one line
  whose `**` opens before the first and closes after the second; both rows
  drop it.
- `a_stale_read_finds_its_own_code`: a read at another stamp whose code block
  moved; the hit in it shows its bytes.

S2, in `crates/app/src/vault_search/tests.rs`: one keystroke test whose note
holds `1. **Buy** milk for the *plan*`; `listed` prints `1: Buy milk for the
plan`.

S3, in `crates/index/src/context_tests.rs`:

- `a_link_keeps_its_syntax_and_the_markup_around_it_drops`: the eleven lines of
  P7, each with its literal shown text and literal lit link.
- `markup_open_before_a_link_drops`: the twelfth case of P7.
- `two_links_on_one_line_each_get_their_context`: `link_contexts` on one line.

S3, in `crates/app/src/backlinks/tests.rs`: a source line
`> Back to *[[index]]* after lunch.`; the row reads `Back to ` before the link
and `[[index]] after lunch.` from it.

## Slices

Vertical. Each slice ends green on `./scripts/check.sh` and runs the mutants
command on its diff:
`git diff main... > $TMPDIR/m.diff && cargo mutants -j 4 --in-diff $TMPDIR/m.diff`.

- **S1. Code ranges, and `⌘P` code lines as written.** `code_text`,
  `links_and_code` and `Source.code` (P11); the cache format 4. Move `Plain`
  and `line_around` (P3), add `shown` and `shown_all` with the code lookup and
  `LINE_BYTES` 256 (P10). `Candidate.code` and `code_of`; `excerpt` takes the
  code ranges. The S1 tests. A quick-open scene row on a code line, rendered
  and looked at. The footprint and the cache size before and after.
- **S2. `⌘⇧F` rows.** `note_hits` builds its hits through `shown_all` with
  `code_of` (P2, P10, P11). Fix the doc comments of `hit.rs` (P8). The S2
  tests. A `vault-search-results` scene line with markup and one in a code
  block, rendered and looked at. A `search` bench case of one dense line with
  100 hits, and one of 1,000 dense lines.
- **S3. Backlink rows.** `link_contexts` over `shown_all` (P10). Fix the doc
  comments of `context.rs` (P8). The S3 tests. A `backlinks` scene line with
  markup, rendered and looked at.

Why code comes first: `⌘⇧F` shows a code line correctly today, as raw bytes.
A plain `⌘⇧F` without P11 would show `let v = abc;` there, a wrong text in a
list that has none now. S1 fixes the one list that is wrong today, `⌘P`, and
lays the data S2 needs. S3 needs no code ranges (P11), only S1's move. Each
slice is one PR; S2 and S3 depend on S1, not on each other.

## Deferred

- **Wikilinks as their alias.** See the first open question.
- **A word with `_` inside it.** `the _team_ wins` gives no hit for `team`.
  Out of scope; it gets its own plan, on `is_word`
  (`crates/index/src/words/fold.rs:30-36`).
- **The dense floor of (ii).** About 6 ms more on a background search whose
  1,000 hits all sit on distinct markup-dense lines. Only fewer parsed bytes
  lower it (see "Cost"). Trigger: S2's 1,000-line bench case, or a measured
  search on real notes, adds more than 10 % with lines shorter than 1 KiB.

## Open questions

1. **Should a backlink row show a wikilink as its alias or target, without the
   brackets?** That changes `markup` or adds a second rule (P1). Recommended:
   no, keep the syntax in this design. The link is the lit part, and the
   brackets tell which link it is.
2. **Should the picker's accessibility label (`label` in
   `vault_search/rows.rs`) read the plain form too?** Recommended: yes. It
   follows from the context with no code (P9).
3. **Should a hit that is all markup (`1`, `x` in a task box) be listed?**
   Recommended: yes, unlit, as `⌘P` does (P6). The count stays true and the hit
   still opens at its source.
4. **Should `⌘P`'s preview keep a 1 KiB first line while the parse radius is
   256 bytes?** Recommended: no. One constant keeps one rule, and a 256-byte
   line probably fills the row already (P10).

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
screen of rows instead of 1,000 hits. It still needs the widened context and
the code ranges from `index`, so it would move the parse, not remove the
index's part. On `⌘⇧F` the parse runs off the foreground already ("Cost"),
so drawing-time parsing would move it onto the frame.

**Drop first, then cut.** See P5.

**One parse per hit, a line-keyed map, a window-only parse, 1 KiB.** See P10.

**Code ranges at search time (a), a hand-written prefilter (c), stored ranges
on a stale read.** See P11.

## Deviations from Zed

Zed's Text Finder shows the source line unchanged. `render_matched_line`
(`vendor/zed-reference/crates/search/src/text_finder/delegate.rs:1342-1427`)
takes a bounded window around the match, trims the indent when the window
starts its line, and adds syntax highlights over the source text. Its files
are code, so the source is what a user reads. Inkra's notes are prose, so the
row drops the markup, as the user decided for `⌘P`. The window's indent trim
matches Zed's.
