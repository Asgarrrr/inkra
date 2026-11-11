# Vault search ranked by the BM25 score

Status: **proposed 2026-11-11.** Measured on the throwaway branch
`spike/ranked-vault-search`, which never merges.

It answers the first half of roadmap item 9 of "Index and search: the
2026-09-27 audit" (`docs/roadmap.md:306`): "List groups by the BM25 score
`⌘P` already computes". It amends the vault search integration spec
(`2026-09-28-vault-search-integration-design.md:113-118`), where groups come
"in vault order".

## Why

`⌘⇧F` lists the notes that hold every term in vault order: folders first,
then by name. `IndexSnapshot::candidates` walks the notes in that order and
keeps each one whose words hold every term (`crates/index/src/query.rs:39-68`).
Nothing after it sorts (V2).

The roadmap records the effect on a 50,000-note Simple English Wikipedia
vault (`docs/roadmap.md:306`). `speed of light` put the note `1,000,000`
first. `black hole` put `1972` first. The `0-9` folder of year notes comes
first in vault order.

The search stops at 1,000 hits (`crates/app/src/vault_search/mod.rs:36`,
`crates/index/src/search/text.rs:113`). On a large vault, the list therefore
holds the first notes in vault order, not the best ones.

`README.md:21-22` claims "`⌘⇧F` vault search ranked by BM25". That is not
true today.

The score already exists. `⌘P` ranks a text match by BM25
(`2026-09-28-ranking-idf-design.md`, R1). `Lookup::score` returns one term's
weight (`crates/index/src/words/lookup.rs:51-59`).

## Design

### V1. The score

A note's score is the sum, over every query term in term order, of
`Lookup::score(term, ordinal, hit)`. This is R1 of the ranking spec, summed
as `rank::place` sums it (`crates/index/src/find/rank.rs:87-92`).

`⌘⇧F` matches text only. Its query has no showing note and no recent list
(`crates/app/src/vault_search/mod.rs:137`). `candidates` keeps a note only
when every term hits its text words (`crates/index/src/query.rs:55-57`). So
every term is placed in the text, and every term adds to the score. A match
in the note's name adds nothing (open question 1).

The score is finite and non-negative: every factor of `bm25` is
(`crates/index/src/words/score.rs:13-16`, ranking spec R2).

### V2. Where the order is made

`IndexSnapshot::candidates` returns the notes best first. No other code
sorts. The spike proved it: with only `candidates` changed, the three app
tests that list rows show the new order on screen (Tests).

The chain that keeps the order:

- `Reader::stream` yields "Each candidate with its read, in the order given"
  (`crates/app/src/index/reads.rs:27`). It uses `buffered(READERS)`, which
  keeps the input order (`crates/app/src/index/reads.rs:59`).
- `TextSearch::read` pushes one `NoteHits` per read, in read order
  (`crates/index/src/search/text.rs:105-107`).
- `Results::new` walks `searched.notes` in order and pushes a header, then
  its hits (`crates/app/src/vault_search/rows.rs:57-66`).

Two readers do not depend on the order:

- The stale list. `TextFound::stale` is "In read order"
  (`crates/index/src/search/text.rs:59`). Its one reader is
  `IndexSync::stale` (`crates/app/src/vault_search/mod.rs:190-192`,
  `crates/app/src/index.rs:264-266`). It reaches `Loads::stale`, which queues
  a load for each indexed note not yet queued
  (`crates/index/src/live/loads.rs:106-111`). The order only decides which
  reload runs first. No result depends on it.
- The selection restore. `HitKey` is a note and a start offset
  (`crates/app/src/vault_search/rows.rs:25-30`). A refresh finds the kept key
  by a linear search over the rows (`crates/app/src/picker/runs.rs:202`). A
  header's count is its own note's hit count
  (`crates/app/src/vault_search/rows.rs:61`, `:84-101`). The `digits` column
  width is a maximum over all rows (`rows.rs:64`). None of them depend on
  vault order.

### V3. Ties, and a total order

`candidates` collects one `(Reverse<Weight>, order)` key per kept note. It
sorts the keys with `sort_unstable`, then builds each `Candidate` from its
`order`.

- `order` is the note's vault position, and it is unique. So the key is a
  total order: equal scores keep vault order.
- A total order gives one result, whatever the sort algorithm. An unstable
  sort is therefore as deterministic as a stable one.
- `Rank` uses the same rule: "the unique `order` makes it total"
  (`crates/index/src/find/rank.rs:12-25`).

This corrects the plan's proposal: a stable `sort_by` over
`(f64, Candidate)`. Both give the same order. The proposal moves 64-byte
elements and costs 1.73 ms more than today on 50,000 candidates. The
correction moves 16-byte keys and costs 0.96 ms more (Cost). A `Candidate`
holds an `Arc<str>` path and a stamp (`crates/index/src/search/text.rs:19-25`).

The `panic!` for a note that holds words but was never loaded
(`crates/index/src/query.rs:61-63`) moves to the step that builds each
`Candidate` from its `order`. The rule does not change.

### V4. The limit

`LIMIT` stays 1,000 hits (`crates/app/src/vault_search/mod.rs:35-36`). It
counts hits, not notes (`crates/index/src/search/text.rs:106`, `:113`). The
reads stop once 1,000 hits exist, so the limit now keeps the best notes.

`limit_reached` keeps its rule: at least `limit` hits exist
(`crates/index/src/search/hit.rs:45`, `crates/index/src/search/text.rs:132`).
After the change it also means: the notes cut are the lowest ranked. A note
gives at most `HITS_PER_NOTE`, 100 hits (`crates/index/src/search/run.rs:11`).
So a list that reaches the limit holds at least 10 notes.

The sort stays a full sort, not a top-k by note count. A candidate can give
zero hits: a text rewritten since its load may no longer hold the terms
(`crates/index/src/search/text.rs:104-108`), and a gone note gives none
(`:110`). So no fixed number of notes is sure to reach 1,000 hits.

### V5. `Weight`

`Weight` moves from `find/rank.rs` (`crates/index/src/find/rank.rs:113-142`)
to `words/score.rs`, beside `bm25`, as `pub(crate)`. `Rank` and `candidates`
both order by `Reverse<Weight>`. One implementation of the score's ordering
serves both searches.

Rejected: sort on `f64::total_cmp` directly in `candidates`. That writes the
ordering twice, and it drops the `debug_assert!` that the score is finite
and non-negative (`crates/index/src/find/rank.rs:117-122`).

The spike sorted on `f64::total_cmp` directly. `Weight::cmp` is that same
call (`rank.rs:124-128`), so the cost should not change. Not measured with
`Weight` itself.

### V6. Docs and names that say "vault order"

These become false and change in the build:

- `crates/index/src/query.rs:39`: the doc of `candidates`. The plan's list
  missed it.
- `crates/index/src/search/hit.rs:43`: `/// In vault order.` on
  `Searched::notes`.
- `crates/index/src/search/text.rs:1`: the module doc.
- `crates/index/src/search/model_tests.rs:48`: the doc of `model`, "over
  `texts` in order".
- `crates/app/src/vault_search/tests.rs:146`: the message
  `"vault order: folders first"`.
- `crates/app/src/index/tests.rs:153`: the doc of the test helper `found`,
  which calls `candidates`. Its 26 call sites passed under the spike, so each
  one lists one note or notes with equal scores. Its doc becomes "best
  first".
- `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md:117`:
  "in vault order". The build adds an amendment note that points here.

`TextFound::stale` stays "In read order" (`text.rs:59`): still true.

## Cost

Machine: Apple M5 Pro, 18 cores, 48 GiB (`sysctl machdep.cpu.brand_string
hw.ncpu hw.memsize`). `--quick`, run 2026-11-11. "Before" is `main` at
`8ef653b5`. "After" is the spike as committed: a stable sort over
`(f64, Candidate)`.

The `search` bench is 50,000 synthetic notes in memory, one core: the
candidates, then each text searched (`crates/index/benches/search.rs:132-159`).
Command: `cargo bench -p inkra-index --bench search -- --quick`.

| `search` case | Before | After |
|---|---|---|
| rare word, `zanzibar`, 50 hits | 0.90 ms | 1.01 ms |
| common word, `projet`, 1,000 hits | 1.31 ms | 2.80 ms |
| 1 char, `e`, 1,000 hits | 1.09 ms | 2.78 ms |
| no match, `projetzz` | 0.30 ms | 0.44 ms |
| folded word, `ete` | 1.84 ms | 2.98 ms |
| 1 accented char, `é` | 1.22 ms | 2.76 ms |
| accented word, `réunion` | 2.29 ms | 3.31 ms |
| two words, `projet réunion` | 1.70 ms | 3.12 ms |
| 1 char twice, `e e` | 1.33 ms | 2.83 ms |

The `long lines` and `scripts` groups have one to ten notes, so the sort does
nothing there. They moved −12 % to +3 % between the two runs: noise.

`candidates` alone: a bench group added on the spike branch only
(`candidates/50k notes`, commit `ab8d75bb`). Command: `cargo bench -p
inkra-index --bench search -- --quick "candidates/"`. Main and the spike ran
in two alternating rounds; the table gives round 1, round 2 agrees within
2 %. Two more variants ran once each:

| `candidates` case | Main | Spike, stable sort of `(f64, Candidate)` | No sort | `(f64, order)` keys, `sort_unstable_by` |
|---|---|---|---|---|
| `e`, 50,000 candidates | 0.73 ms | 2.47 ms | 1.09 ms | 1.69 ms |
| `e e`, 50,000 | 0.73 ms | 2.50 ms | 1.09 ms | 1.73 ms |
| `projet`, 50,000 | 0.64 ms | 2.36 ms | 0.99 ms | 1.62 ms |
| `projet réunion`, 49,969 | 0.76 ms | 2.47 ms | 1.14 ms | 1.82 ms |
| `zanzibar`, 50 | 0.32 ms | 0.48 ms | 0.48 ms | 0.48 ms |

- The worst case adds 1.73 ms with the spike's sort, and 0.96 ms with V3's
  keys. Both stay under the plan's 2 ms stop line.
- The sort itself is 1.37 ms of the spike's 1.73 ms. Scoring and the extra
  vector cost the rest, about 0.36 ms.
- `zanzibar` and `projetzz` keep 50 and 0 candidates, yet both gain about
  0.15 ms. The sort cannot explain it; the loop's new shape probably does.
  Not profiled; the build profiles it before it optimizes.
- The keystroke budget is 16 ms for every `search` case
  (`2026-09-28-search-word-start-design.md:250-256`). The slowest case after
  the change is 3.31 ms.

The `read_search` bench reads a `bench_vault` vault of 50,000 notes on disk.
It searches for a term whole in 1,000 notes, and reaches the limit. Command:
`INKRA_BENCH_VAULT=<dir> cargo bench -p inkra-index --bench read_search --
--quick`. The vault came from `crates/app/examples/bench_vault.rs`, built
with `rustc --edition 2024 -O` and run with `TMPDIR` set to a private
directory: `bench_vault` uses only `std` and writes under
`std::env::temp_dir()` (`bench_vault.rs:339`).

| `read_search` | Main | Spike |
|---|---|---|
| `resident`, texts in memory | 8.6, 9.5 ms | 8.5, 8.7 ms |
| `read4`, 4 threads read from disk | 23.6, 23.9 ms | 22.7, 22.9 ms |

Each cell is two alternating runs. The first pair of runs, not alternated,
gave 27.9 ms and 16.2 ms for `read4`: the warm page cache moves this figure
by more than the change does. So the warm figures show no measurable change.

**The first-hits risk.** Reads now leave vault order. A note's files sit in
folders, so vault order probably reads neighbouring files. On a cold cache,
the first hits may land later. `read_search` cannot show it. It reads warm,
and `read4` reads each thread's share in a contiguous run
(`crates/index/benches/read_search.rs:56-83`), not as `Reader::stream` does.

The build measures it in the app. Run a release build on the 50,000-note
vault of `docs/benchmarks.md` (`docs/benchmarks.md:34-38`), cold after
`sudo -n purge` (`docs/benchmarks.md:106`). Read the release log line
`search: first hits …, landed …, read N`
(`crates/app/src/vault_search/mod.rs:50-52`, printed at `:187-189`). Compare
main and the build on the same queries, warm and cold.

## Tests

**The oracle.** `model` in `crates/index/src/search/model_tests.rs:49`
ranks the notes that hold every term by BM25. It recomputes `df`, `N`, `len`
and `avg` from its own texts, and calls `idf` and `bm25`, as the `find`
model does (`crates/index/src/find/model_tests.rs:19-32`, `:92`, `:108-109`).
It never calls `Lookup`: a model that calls `Lookup::score` checks the code
against itself. It adds the terms in term order, so its sum equals the
code's to the last bit (ranking spec R1). Ties keep the order of `texts`,
which is vault order. Then it applies the limit as today.

**Tests that failed under the spike.** Each one asserts vault order. Every
other test passed: `cargo test --workspace --no-fail-fast`.

- `inkra-index`, `search::tests::hits_come_in_vault_order_and_skip_notes_not_loaded`
  (`crates/index/src/search/tests.rs:67`). It now lists `plan plan` before
  `x plan`. The build renames it and asserts the ranked order.
- `inkra-index`, `search::model_tests::the_two_boxes_agree` (`:191`). It is
  a proptest: it failed in two of three runs, on the draws.
- `inkra-index`, `search::model_tests::hits_equal_the_model_past_the_per_note_cap`
  (`:199`).
- `inkra-index`, `search::model_tests::the_drawn_cases_hold_notes_matched_and_notes_missed`
  (`:214`).
- `inkra-app`, `vault_search::tests::typing_lists_a_header_then_the_hits_of_each_note`
  (`crates/app/src/vault_search/tests.rs:127`).
- `inkra-app`, `vault_search::tests::rows::each_row_label_reads_as_its_row`
  (`crates/app/src/vault_search/tests/rows.rs:7`).
- `inkra-app`, `vault_search::tests::refresh::a_refresh_during_loads_keeps_the_selected_hit`
  (`crates/app/src/vault_search/tests/refresh.rs:24`). The kept hit is still
  b's second hit; only its row index changes.

The three model tests fail on order only. With the notes compared by id and
no limit, all three passed under the spike (a local edit, not committed).
The three app failures are reorderings of the same rows.

**New tests.**

- App: two notes. The later one in vault order holds the rare term more
  often, and lists first. It fails on today's code.
- Index: equal scores keep vault order. Two notes with the same text list in
  vault order.
- Index: the limit keeps the best notes. With a limit of one note's hits, the
  best note is kept, not the first one.
- `cargo mutants` on the build's diff, from the repo's command
  (`CLAUDE.md`, "mutants"): zero missed.

## Slices

Each slice holds data, behavior and a test.

- **S1, `inkra-index`.** `Weight` moves to `words/score.rs` (V5).
  `candidates` returns the notes best first (V1, V3). The oracle ranks
  (Tests). The index tests change, and the two new index tests land. The
  docs of V6 in `inkra-index` change. `cargo mutants` on the diff.
- **S2, the app.** The three app tests change, and the new app test lands.
  The docs of V6 in `inkra-app` and the integration spec change. A shot of
  the list with ranked groups. The cold first-hits measure (Cost).
  `README.md:21-22` claims "ranked by BM25" again. Roadmap item 9 is struck
  with the date.

S1 alone changes the order on screen, so S1 and S2 merge together, or S2
directly after S1.

## Deferred

| Item | Trigger |
|---|---|
| Line order inside a note: list first the lines that hold the most terms | A ranked note still shows weak lines first on the Wikipedia vault |
| Light a common term only in a line that holds a rarer one (`1,000,000` showed lines holding only `of`) | Same, after line order |
| Proximity re-ranking of the top notes, the general form (`docs/roadmap.md:306`) | Line order is not enough |
| A top-k or a parallel `candidates` | The worst case grows past 2 ms on a real vault |
| Streaming results (`2026-09-24-search-design.md:1263`) | Its own trigger: first results over 100 ms |

Streaming gets harder with ranks. That row appends groups in vault order as
reads land. With ranked groups, a later read can outrank a shown group. It
would then need to insert above shown rows, or wait for the order to be
final.

## Open questions for the maintainer

1. **Does a note whose name holds the terms deserve a boost in `⌘⇧F`?**
   Recommended: no. `⌘P` is the name search: its `Name` group comes before
   any text match (`crates/index/src/find/rank.rs:95-100`). `⌘⇧F` reads
   only the text, and the query it builds has no name context
   (`crates/app/src/vault_search/mod.rs:137`). A boost would add a second
   score that the oracle must model too.
2. **Is a first-hits regression on a cold cache acceptable, and up to what
   figure?** Recommended: yes, up to the search spec's requirement, results
   within 100 ms of the debounce's end at 50,000 notes
   (`2026-09-28-search-word-start-design.md:252-253`). Evidence: the warm
   figures show no change (Cost). The cold figure is not measured; S2
   measures it.
3. **Should the header show anything about rank?** Recommended: no. The
   order already shows it. `⌘P` shows no score either. A score number means
   nothing to a reader.
4. **Should the spike's 0.15 ms on queries with few candidates block S1?**
   Recommended: no. It is under 1 % of the 16 ms budget. S1 profiles it
   before it optimizes.

## Rejected alternatives

**Sort `Searched` after the reads.** The limit would still cut in vault
order: the reads stop at 1,000 hits (`crates/index/src/search/text.rs:113`),
so the best notes past the cut are never read. And the scores live in the
`Lookup`, which the reads do not hold.

**A separate scorer for `⌘⇧F`.** `Lookup::score` already gives the weight.
A second scorer would be a second formula that the oracle must check, and
the two searches could disagree on one note.

**A stable sort over `(f64, Candidate)`.** It gives the same order as V3,
and costs 0.77 ms more on the worst case (Cost).

**A top-k of the best 1,000 notes.** A note can give zero hits (V4), so a
top-k could stop short of the limit while better notes exist.

**Rank inside `fan_out` over parallel ranges, as `find_notes` does.**
`candidates` runs on one core and costs at most 1.82 ms after V3. Parallel
ranges add a merge for a cost that fits the budget. Deferred, with a trigger.

## Deviations from Zed

Zed's project search does not rank by relevance. Its results arrive in path
order.

- Local search walks the worktrees sorted by id
  (`vendor/zed-reference/crates/project/src/project_search.rs:123-127`).
- `maintain_sorted_search_results` forwards the matching paths in the order
  of the candidates, and stops at the file limit (`project_search.rs:532-556`).
- `ensure_matched_ranges_are_reported_in_order` forwards each buffer's
  ranges in order, and sends `LimitReached` past 5,000 files or 10,000 ranges
  (`project_search.rs:166-167`, `:636-663`).
- The view keeps its excerpts sorted by `PathKey` as results stream in
  (`vendor/zed-reference/crates/search/src/project_search.rs:656`, `:949`).

**Deviation:** Inkra ranks by BM25 and keeps no path order. Reason: Zed
searches a project, where a path names a module and a reader scans by
folder. Inkra searches a vault of notes, where the question is which note is
about the terms. The word index already holds the term counts and note
lengths, so the score costs one sum per candidate (Cost). Zed has no such
index: it scans files as it goes, and can only stream in path order.
