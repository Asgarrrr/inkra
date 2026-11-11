# Vault search ranked by the BM25 score

Status: **proposed 2026-11-11, revised twice the same day.** Measured on the
throwaway branch `spike/ranked-vault-search`, which never merges. The first
revision measured V3's keys and two lazy orders, and made the cold measure a
blocking criterion of S2. The second revision measures reads past the first
block, picks doubling blocks by those figures, builds the owning `Ranked`,
and gives the cold threshold an exact rule.

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

`IndexSnapshot::candidates` yields the notes best first. No other code
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

`candidates` collects one key per kept note: `(Reverse<Weight>, order)`.

- `order` is the note's vault position, and it is unique. So the key is a
  total order: equal scores keep vault order.
- A total order gives one result, whatever the algorithm that orders it. An
  unstable sort, a heap and a partial sort therefore all give the same
  sequence. V4 relies on this.
- `Rank` uses the same rule: "the unique `order` makes it total"
  (`crates/index/src/find/rank.rs:12-25`).

This corrects the plan's proposal: a stable `sort_by` over
`(f64, Candidate)`. It gives the same order, but moves 64-byte elements. A
key is 16 bytes. A `Candidate` holds an `Arc<str>` path and a stamp
(`crates/index/src/search/text.rs:19-25`).

### V4. The lazy order: sorted blocks that double, from 64

`candidates` sorts no more keys than the reads consume. It holds the keys in
one vector and orders them in blocks. The first block holds 64 keys, and each
next block holds twice the one before: 64, 128, 256, 512, and so on.

1. When the next key is past the sorted part, it calls
   `select_nth_unstable(block - 1)` on the rest. That moves the `block` best
   keys of the rest to its front, in O(rest).
2. It sorts those keys with `sort_unstable`, and doubles `block`.
3. It yields them one by one, and builds each `Candidate` from its `order`
   only when yielded.

A rest of `block` keys or fewer is sorted whole. The total order of V3 makes
the sequence equal to a full sort's, ties included, whatever the block sizes.
The proptest of Tests proves it for fixed and doubling blocks. The spike
holds it as `Blocks` and `Order` (`crates/index/src/query.rs:220`, `:228` on
the spike).

**The bound by construction.** Let n be the candidates, and r the notes the
reads consume.

- A selection visits at most the rest, so at most n keys.
- Doubling from 64 makes k = ceil(log2(r/64 + 1)) selections. The sorted keys
  sum to 64 · (2^k − 1), which is less than 2r + 64.
- So the order work is at most k · n key visits plus sorts of fewer than
  2r + 64 keys in all. Per note read, the selection work is at most k · n / r visits,
  and k grows with log r, not with r.
- For r = 1,000: 5 selections, 1,984 keys sorted. At n = r = 50,000 the bound
  is 10 selections.
- A fixed block of 64 makes ceil(r/64) selections: 16 for r = 1,000, 782 for
  r = 50,000. Its selection work grows linearly with r, about n/64 visits
  per note read.

How far r goes. The search stops at 1,000 hits, and each read whose text
still holds the terms gives at least one hit. So r exceeds 1,000 only
when reads give no hits: a text rewritten or gone since its load (below). In a search that never reaches the
limit, every candidate is read, so r = n and every design orders all n keys.

Why the order stops early. `Reader::stream` feeds `buffered(READERS)`, which
pulls the next candidate only while fewer than 4 reads are in flight
(`crates/app/src/index/reads.rs:59`). The search loop breaks once
`TextSearch::read` returns false (`crates/app/src/vault_search/mod.rs:161-169`).
So the iterator stops being pulled at the limit, and the unread keys are
never sorted. On the bench's corpus, the nine word cases read at most 38
notes before the limit (Cost, R1 counts): one selection and one sort of 64
keys serve them. The `kumquat` case reads 1,000 notes: 5 selections.

Why no fixed top-k. A candidate can give zero hits: a text rewritten since
its load may no longer hold the terms (`crates/index/src/search/text.rs:104-108`),
and a gone note gives none (`:110`). So no fixed number of notes is sure to
reach 1,000 hits. The block order has no fixed count: past each block, the
next `select_nth_unstable` orders a block twice as large. It stays exact
whatever the reads consume.

The weight cannot be computed for the notes read only. A best-first order
must compare every candidate's score, so each kept note is scored once. Only
a bound-based pruning (MaxScore, WAND) could skip scores, at the cost of a
per-term upper bound in the word index (Deferred). The lazy order skips
what it can skip exactly: the 64-byte `Candidate` of every note not read.

**What `candidates` returns.** An owning iterator, `Ranked`, that is
`Send + 'static`. It holds the key vector, the block cursor, and clones of
the names and the corpus that build each `Candidate`. It replaces
`Option<Vec<Candidate>>` by `Option<Ranked>`: `None` still means cancelled.
The spike builds it in `inkra-index` (`crates/index/src/query.rs:288`, on
the spike). A `const` item asserts `Send + 'static` at compile time (`:294`),
and the test `a_ranked_order_outlives_its_snapshot` drops the index before
it drains a `Ranked`.

What its clones cost, on `main`:

- `NamesSnapshot` holds an `Arc<[Name]>` and an `Arc<NameWords>`. Its doc:
  "Cloning is an `Arc` bump" (`crates/index/src/names.rs:20-25`).
- `CorpusSnapshot` holds an `Arc<[EntryId]>`, an `Arc<[Chunk]>` and a
  `usize`. Its doc: "Cloning is an `Arc` bump"
  (`crates/index/src/corpus.rs:113-119`).
- `IndexSnapshot` holds both, and its accessors return references
  (`crates/index/src/index.rs:323-331`, `:346-352`).

So a `Ranked` costs four `Arc` increments once per query, and no copy of a
name or a text. The bench measured the owning form against the borrowing one
(Cost): equal within the spread of runs.

**Does `Reader::stream` need `'static`?** Its signature returns
`impl Stream<Item = (Candidate, Read)> + Send + 'static`
(`crates/app/src/index/reads.rs:27-34`), and `stream_with` does the same
(`:37-60`). Each read task owns its `Candidate` (`:54-57`); the iterator
itself is pulled on the consumer's side.

- Vault search would do with a borrow. `search` drains the stream inside its
  own future (`crates/app/src/vault_search/mod.rs:157-169`), and it already
  borrows `&IndexSnapshot` (`crates/app/src/picker/runs.rs:57-62`). The
  spawned task owns that snapshot (`runs.rs:160-174`).
- Quick open needs `'static`. It moves its stream into `cx.spawn(...)` and
  detaches it (`crates/app/src/quick_open/list.rs:146-176`). The reads test
  moves it into a background task too (`crates/app/src/index/reads_tests.rs:27`).

Decision: the owning `Ranked`. One `'static` signature keeps serving every
caller, and the bench shows no cost for owning. Rejected: a borrowing
`impl Iterator + '_` from `candidates`, with `stream` generic over the
iterator's lifetime. It saves four `Arc` increments per query, and it makes
`stream`'s bound depend on its caller.

What changes for its callers:

- `Reader::stream` (`crates/app/src/index/reads.rs:27-34`) takes
  `impl IntoIterator<Item = Candidate>` whose iterator is `Send + 'static`,
  in place of a `Vec`, and stops collecting it (`:32`). `stream_with`
  (`:37-60`) takes the same bound over its items. `futures::stream::iter`
  already accepts any `IntoIterator`. A `Vec` still satisfies the bound, so
  `quick_open/list.rs:146` and `index/reads_tests.rs:27` do not change.
- `vault_search/mod.rs:157-160` passes the iterator on. No other line moves.
- The app test helper `candidates` (`crates/app/src/index/tests.rs:138-150`)
  collects into a `Vec`, so its seven call sites do not change.
- The benches `read_search.rs:63`, `:92` and `search.rs:139` collect, or
  iterate the iterator. `read4` needs a slice to share between threads, so it
  collects.

`TextSearch` does not change. It takes one `&Candidate` per read
(`crates/index/src/search/text.rs:90`).

The `panic!` for a note that holds words but was never loaded
(`crates/index/src/query.rs:61-63`) moves to the step that builds a
`Candidate` from its `order`. The rule does not change, but it now fires
only for a yielded note. A broken note past the limit no longer panics. The
`find` path keeps its own check, so the invariant is still exercised on each
`⌘P` keystroke.

### V5. The limit

`LIMIT` stays 1,000 hits (`crates/app/src/vault_search/mod.rs:35-36`). It
counts hits, not notes (`crates/index/src/search/text.rs:106`, `:113`). The
reads stop once 1,000 hits exist, so the limit now keeps the best notes.

`limit_reached` keeps its rule: at least `limit` hits exist
(`crates/index/src/search/hit.rs:45`, `crates/index/src/search/text.rs:132`).
After the change it also means: the notes cut are the lowest ranked. A note
gives at most `HITS_PER_NOTE`, 100 hits (`crates/index/src/search/run.rs:11`).
So a list that reaches the limit holds at least 10 notes.

### V6. `Weight`

`Weight` moves from `find/rank.rs` (`crates/index/src/find/rank.rs:113-142`)
to `words/score.rs`, beside `bm25`, as `pub(crate)`. `Rank` and `candidates`
both order by `Reverse<Weight>`. One implementation of the score's ordering
serves both searches. The spike did this move, and every figure of Cost
uses `Weight`.

Rejected: sort on `f64::total_cmp` directly in `candidates`. That writes the
ordering twice, and it drops the `debug_assert!` that the score is finite
and non-negative (`crates/index/src/find/rank.rs:117-122`).

### V7. Docs and names that say "vault order"

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
hw.ncpu hw.memsize`). Run 2026-11-11. Other processes loaded the machine:
`uptime` gave a load average of 13.53 before run 1, 10.67 after run 1, 6.88
after run 2, and 7.02 after run 3. No other heavy command of this work ran
meanwhile.

The `search` bench is 50,000 synthetic notes in memory, on one core
(`crates/index/benches/search.rs:132-159` on `main`). The spike adds groups
and two cases to it (commit `148ddc83`). Every design runs in one binary, so
they interleave within each run:

- `main`: today's `candidates`, vault order.
- full: V3's keys `(Reverse<Weight>, order)`, `sort_unstable`, then every
  `Candidate` built.
- b64: fixed blocks of 64, the first revision's V4, owning `Ranked`.
- b256: fixed blocks of 256, owning `Ranked`.
- dbl: doubling blocks from 64, this revision's V4, owning `Ranked`.
- b64 borrowed: fixed blocks of 64, a borrowing iterator over the snapshot.

The heap of the first revision is dropped from the bench table: it lost to
b64 in 8 of 9 keystroke cases there (Rejected alternatives). The proptest
keeps it as a reference.

The two new cases cross many blocks. Each note's text gets one word that
draws nothing from the generator, so the nine first cases keep their texts:

- `kumquat`: every note holds it once. 50,000 candidates, and the limit
  takes 1,000 reads, 16 blocks of 64. This is the most reads one search
  makes when every read gives a hit (V4, the bound).
- `quince`: one note in 60 holds it once. 834 candidates, 834 reads, 834
  hits, so the limit is never reached and every candidate is read. N must
  stay under 1,000: with one hit per note, the 1,000th read would reach the
  limit. One note in 60 gives 834 at 50,000 notes, close to that ceiling.
  It makes 14 blocks of 64, and 4 blocks when doubling.

Command, run 3 times back to back:

```
cargo bench -p inkra-index --bench search -- --quick "designs/|first read/"
```

**The cost table.** Each figure is the median of the 3 runs' criterion
estimates, in ms. "Keystroke" is `designs/50k notes`: the candidates, then
each text searched until the limit, as `search.rs:132-159` does. "First
read" is `first read/50k notes`: `candidates`, then the first candidate
yielded, R1 (a). The bench asserts that every ranked design yields the full
sort's order on all eleven cases.

| case | candidates | reads, ranked | keystroke: main | full | b64 | b256 | dbl | b64 borrowed | first read: main | full | b64 | b256 | dbl | b64 borrowed |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `zanzibar` | 50 | 50 | 1.004 | 0.957 | 0.955 | 0.959 | 0.953 | 0.990 | 0.428 | 0.439 | 0.429 | 0.469 | 0.447 | 0.453 |
| `projet` | 50,000 | 19 | 1.291 | 2.286 | 1.060 | 1.057 | 1.048 | 1.041 | 0.907 | 1.748 | 0.705 | 0.719 | 0.724 | 0.737 |
| `e` | 50,000 | 12 | 1.189 | 2.176 | 1.044 | 1.034 | 1.023 | 1.029 | 0.933 | 2.761 | 0.800 | 0.802 | 0.794 | 0.796 |
| `projetzz` | 0 | 0 | 0.375 | 0.378 | 0.379 | 0.378 | 0.379 | 0.379 | 0.397 | 0.394 | 0.388 | 0.399 | 0.415 | 0.380 |
| `ete` | 49,963 | 38 | 1.712 | 2.457 | 1.214 | 1.236 | 1.238 | 1.237 | 0.749 | 1.760 | 0.649 | 0.647 | 0.645 | 0.645 |
| `é` | 50,000 | 12 | 1.170 | 2.250 | 1.028 | 1.027 | 1.020 | 1.062 | 0.892 | 1.857 | 0.784 | 0.787 | 0.825 | 0.848 |
| `réunion` | 49,969 | 38 | 2.178 | 2.916 | 1.553 | 1.658 | 1.843 | 1.690 | 0.743 | 1.756 | 0.647 | 0.660 | 0.653 | 0.655 |
| `projet réunion` | 49,969 | 19 | 1.558 | 2.538 | 1.461 | 1.437 | 1.359 | 1.361 | 0.912 | 1.771 | 0.862 | 0.848 | 0.841 | 0.848 |
| `e e` | 50,000 | 12 | 1.203 | 2.217 | 1.035 | 1.026 | 1.025 | 1.025 | 0.891 | 1.846 | 0.787 | 0.786 | 0.785 | 0.785 |
| `kumquat` | 50,000 | 1,000 | 11.112 | 5.575 | 5.294 | 4.618 | 4.707 | 5.362 | 0.747 | 1.529 | 0.668 | 0.712 | 0.645 | 0.646 |
| `quince` | 834 | 834 | 9.070 | 8.999 | 9.005 | 9.075 | 9.201 | 9.184 | 0.444 | 0.444 | 0.435 | 0.434 | 0.433 | 0.434 |

The nine first cases read at most 38 notes, so every lazy design runs one
selection and one sort of 64, or more for b256. Their differences there are
the spread of runs. On `réunion`, dbl's three runs gave 1.968, 1.603 and
1.843 ms for the same code path as b64.

- Every lazy design is at or below `main` on every keystroke case but
  `projetzz` and `quince`, and on every first read but `zanzibar` and
  `projetzz`. Those cases hold 0, 50 or 834 keys; dbl's are covered below.
- The full sort costs up to 1.08 ms more than `main` on the nine first cases
  (`é`, keystroke), and delays the first read by up to 1.83 ms (`e`).
- The owning form against the borrowing one, b64 against b64 borrowed: the
  owning form is 0.179 ms faster at most (`quince`, keystroke) and 0.100 ms
  slower at most (`projet réunion`, keystroke). Neither form is faster in a
  consistent way. So owning costs nothing measurable (V4, the decision).

**The case past the first block, `kumquat`.** It separates the designs by
their number of selections over 50,000 keys: b64 makes 16, dbl 5, b256 4.

- b64: 5.294 ms. dbl: 4.707 ms. b256: 4.618 ms. The 11 selections that b64
  makes beyond dbl cost about 0.59 ms, so about 0.05 ms per selection over
  50,000 keys.
- dbl and b256 differ by 0.09 ms, inside the spread: b256's first run gave
  8.034 ms.
- Every ranked design runs about twice as fast as `main` here, with the same
  1,000 reads. The order work cannot explain it: the full sort is also
  faster. Probable cause: with one occurrence per note, BM25's length term
  puts the short notes first, so the ranked reads scan less text. This is a
  hypothesis. The bench did not count the bytes scanned.

**The choice: doubling from 64.** On the nine first cases it ties with b64
and b256. On `kumquat` it beats b64 by 0.59 ms and ties with b256. Its bound
grows with log r (V4), and a fixed block's grows with r. So dbl is the one
design that is cheapest on both measured shapes and bounded on the shapes
not measured. Rejected: b64, 16 selections at 1,000 reads; b256, 782 / 4 =
196 selections at r = 50,000, and a first block four times larger for
searches that read 12 to 38 notes.

**Where dbl costs more than `main`.**

- `quince`, keystroke: 9.201 against 9.070 ms, +0.131 ms, +1.4 %. Every
  candidate is read, so every ranked design orders all 834 keys; `main`
  orders none. No ranked design avoids that work: a best-first order of the
  notes read must compare every one of them. It is bounded: 3 selections
  and 4 sorts over at most 834 keys. The selections visit at most 2,502
  keys, 20 times fewer than the one selection over 50,000 that the nine
  first cases pay. Full, b64 and b256
  order the same 834 keys and measure 8.999 to 9.075 ms, within
  0.005 ms of `main` or below it. `main`'s own three runs spread from 9.040 to 10.939 ms, dbl's from
  9.116 to 10.033 ms. So the +0.131 ms is probably the spread, not the
  order. In a search that never reaches the limit, the keys to order are at
  most 999 plus the reads that give no hit (V4, how far r goes).
- `projetzz`, keystroke: 0.379 against 0.375 ms, +0.004 ms. It keeps 0
  candidates, so no design orders anything.
- First read, `zanzibar` +0.019 ms and `projetzz` +0.018 ms. They hold 50
  and 0 keys, one sort of at most 64 keys or none. Both sit inside the loop
  variance that "Where the time goes" names.

The `long lines` and `scripts` groups hold one to ten notes, so the order
does nothing there. The first revision measured them as noise, −12 % to
+3 %. This revision did not rerun them.

**Where the time goes** (`profile/50k notes`). These are the first
revision's figures, from its 3 runs; this revision did not rerun the group.
Each step adds work to the step before, over all 50,000 notes. Median of 3
runs, in ms:

| step | `e` | `projet` | `zanzibar` | `projetzz` |
|---|---|---|---|---|
| the lookup alone | 0.223 | 0.131 | 0.019 | 0.000 |
| + the per-note filter, as `main` | 0.681 | 0.705 | 0.483 | 0.487 |
| + the score | 0.853 | 0.755 | 0.491 | 0.556 |
| + the keys pushed | 0.929 | 0.737 | 0.414 | 0.366 |
| + every `Candidate` built | 1.193 | 0.922 | 0.467 | 0.447 |
| `main`'s `candidates` | 0.832 | 0.759 | 0.381 | 0.304 |

- The floor is the filter: about 0.3 to 0.5 ms with few or no candidates. It
  is one `Lookup::ordinal(id)` hash lookup per vault note
  (`crates/index/src/words/store.rs:18`, `:363-365`), 50,000 per query, in
  `main` and in every design.
- The first revision's "no sort" variant cost 1.09 ms against `main`'s
  0.73 ms on `e`. The profile names its parts on `e`: the score adds about
  0.17 ms, pushing the keys about 0.08 ms, and building every `Candidate`
  from the keys about 0.26 ms. These come from other runs than the 1.09 ms,
  so they do not sum to its 0.36 ms exactly. The lazy designs skip the
  last part for the notes not read.
- The first revision's +0.15 ms on `zanzibar` and `projetzz` is not added
  work. Those queries keep 50 and 0 candidates, so the score and the keys
  are almost free. The "filter, as `main`" step does `main`'s work, yet
  measures 0.10 to 0.18 ms above `main`'s `candidates` on the same queries.
  The same loop thus moves by that much with its compiled shape. This is
  probable, not proven: no disassembly was read.

**Before the first read.** `main` builds its whole `Vec<Candidate>` before
the first read, so its first read waits for every candidate too. dbl issues
the first read within +0.019 ms of `main` in every case, and before it in
nine of eleven. The full sort delays it by up to 1.83 ms (`e`).

**R1 counts.** These are deterministic: the bench prints them, and they are
the same in every run and in every ranked design. The bench asserts that
every ranked design yields the full sort's order on all eleven cases.

| case | candidates | reads to the first hit, main / ranked | reads to the limit, main | reads to the limit, ranked |
|---|---|---|---|---|
| `zanzibar` | 50 | 1 / 1 | 50, no limit | 50, no limit |
| `projet` | 50,000 | 1 / 1 | 34 | 19 |
| `e` | 50,000 | 1 / 1 | 18 | 12 |
| `projetzz` | 0 | none | 0 | 0 |
| `ete` | 49,963 | 1 / 1 | 76 | 38 |
| `é` | 50,000 | 1 / 1 | 18 | 12 |
| `réunion` | 49,969 | 1 / 1 | 71 | 38 |
| `projet réunion` | 49,969 | 1 / 1 | 25 | 19 |
| `e e` | 50,000 | 1 / 1 | 18 | 12 |
| `kumquat` | 50,000 | 1 / 1 | 1,000 | 1,000 |
| `quince` | 834 | 1 / 1 | 834, no limit | 834, no limit |

- R1 (b), the reads issued before the first hits land: one in both orders.
  Every candidate holds every term at its load, so the first read gives hits
  unless its text changed since. In the app, `buffered(READERS)` issues up
  to 4 reads at once, in both orders, so up to 4 reads are in flight when
  the first hits land.
- Ranked reads 24 % to 50 % fewer notes to reach the limit on the seven
  word cases that reach it. On `kumquat` every read gives one hit, so both
  orders read 1,000 notes.

**The warm read.** The `read_search` bench reads a `bench_vault` vault of
50,000 notes on disk, warm. It searches for a term whole in 1,000 notes and
reaches the limit. The first revision ran it against the spike's first
commit, a full stable sort. Command: `INKRA_BENCH_VAULT=<dir> cargo bench -p
inkra-index --bench read_search -- --quick`. The vault came from
`crates/app/examples/bench_vault.rs`, built with `rustc --edition 2024 -O`
and run with `TMPDIR` set to a private directory: `bench_vault` uses only
`std` and writes under `std::env::temp_dir()` (`bench_vault.rs:339`).

Over 2 alternating runs each, `resident` (texts in memory) gave 8.6 and
9.5 ms on `main`, 8.5 and 8.7 ms with the stable sort. `read4` (4 threads
read from disk) gave 23.6 and 23.9 ms on `main`, 22.7 and 22.9 ms with the
stable sort.

A first pair of runs, not alternated, gave 27.9 ms and 16.2 ms for `read4`.
The warm page cache moves this figure more than the change does. This
revision did not rerun it with blocks: `read4` collects the candidates to
share them, so it cannot show the lazy order.

### The cold first hits: a blocking criterion of S2

The search spec requires results within 100 ms of the debounce's end at
50,000 notes (`2026-09-28-search-word-start-design.md:252-253`). That
requirement stands as written. This spec adds no tolerance on top of it.

**The argument by construction.** Two things decide when the first hits
land on a cold cache: when the first read is issued, and how many reads it
takes.

- The first read is issued after `candidates` returns its first key. dbl
  matches `main` within 0.019 ms on every case (the cost table), against a
  100 ms requirement.
- The first hits need one read in both orders (R1 counts). Both orders issue
  4 reads at once.
- What differs is which files the first 4 reads open. Vault order opens
  neighbours in one folder. Ranked order opens files in any folder. On a
  cold cache, a read's cost is the file's own metadata and data blocks, and
  neighbours in a folder are not sure to sit near each other on an SSD. So
  the first read probably costs the same in both orders. This is the claim
  the measure checks.
- Past the first hits, ranked reads fewer notes to the limit (24 % to 50 %
  fewer on the bench). So `landed` should not grow.

**The measure.** The maintainer runs it; it needs `sudo` and the app. S2 is
not done until it passes.

1. Build both `main` and S2 in release: `cargo build --release -p inkra-app`,
   one target directory each.
2. Use the Wikipedia vault of `docs/roadmap.md:306` if it is at hand, else
   the 50,000-note `bench_vault` of `docs/benchmarks.md:34-38`.
3. Queries: the nine `search` bench cases, plus `speed of light` and
   `black hole` on the Wikipedia vault.
4. One run: quit the app, `sudo -n purge` (`docs/benchmarks.md:106`),
   launch the build on the vault, wait for the index to settle, open `⌘⇧F`,
   paste the query. Alternate the builds run by run, `main` then S2, so a
   drift of the machine reaches both.
5. Read the release log line `search: first hits …, landed …, read N`
   (`crates/app/src/vault_search/mod.rs:49-52`, printed at `:188-190`). Keep
   the line of the answer to the full query.

**The absolute rule.** Every S2 run's `first hits` is at most 100 ms. One
run above it fails the query. More runs do not undo it.

**The relative rule**, per query, for `first hits` and for `landed` apart.
"The k-th of `main`" is the k-th smallest of `main`'s runs. The median of
an even count is the mean of its two middle values.

1. Take 10 runs per build. The metric passes if S2's median is at or below
   the 9th of `main`'s 10, its second slowest run.
2. Otherwise the result is borderline. Take 20 more runs per build, 30 in
   all, and keep the first 10. The metric passes if S2's median of 30 is at
   or below the 25th of `main`'s 30. Otherwise it fails.

A borderline result never triggers the fallback: only a fail at step 2 does.

`read N` must also be at or below `main`'s on each query that reaches the
limit. It is a count, so it checks the run, not the disk.

Why these counts. The rule compares S2 with `main`'s own spread, so it
assumes no shape of that spread. A simulation drew both builds' runs from one
log-normal spread, at a spread of 10 % and of 30 %, 40,000 trials each
(`rule_sim2.py`, run once, not kept in the repo):

- With no change, a metric fails 0.17 % of the time and needs step 2 4.5 %
  of the time. Over 11 queries and 2 metrics, a build with no change fails
  the measure about 4 % of the time: 1 − 0.9983^22.
- A 20 % slower S2 fails 91 % of the time at a 10 % spread, and 11 % at a
  30 % spread. A 50 % slower S2 fails 100 % and 69 % of the time.
- A first draft passed at the 8th of 10, then at the 15th of 20. It failed
  about 4 % of no-change metrics, so about 60 % of no-change measures. The
  first revision's rule, S2's median at or below `main`'s, fails a
  no-change metric about half the time. Both fail on noise, so the rule
  above replaces them.

These rates hold only if cold runs are independent and drawn from one
spread per build. That was not checked on real cold runs. So a wide spread
can hide a slowdown, and the absolute rule is the backstop.

**The fallback.** If a query fails, S2 raises `READERS` from 4 to 8 for the
ranked stream, and the measure runs again. Cost: up to 4 more reads in
flight past the limit, each one a read whose hits are thrown away, and up
to 4 more open files. More reads in flight hide the latency of scattered
files; the cost is bounded and needs no new code path.

**The STOP rule.** If the measure still fails with 8 readers, S2 stops and
does not merge. The maintainer then chooses with the figures in hand. No
design in this spec accepts a slower first hit.

## Tests

**The oracle.** `model` in `crates/index/src/search/model_tests.rs:49`
ranks the notes that hold every term by BM25. A new helper, `ranked`,
recomputes `df`, `N`, `len` and `avg` from the texts' own words, and calls
`idf` and `bm25`, as the `find` model does
(`crates/index/src/find/model_tests.rs:19-32`, `:92`, `:108-109`). It never
calls `Lookup`: a model that calls `Lookup::score` checks the code against
itself. It adds the terms in term order, so its sum equals the code's to the
last bit (ranking spec R1). It then sorts with a stable `sort_by` on
`total_cmp`, best first, so ties keep the order of `texts`, which is vault
order. Then it applies the limit as today. The spike built it, and the three
model tests pass against it.

**The lazy order.**

- `query::tests::lazy_orders_equal_the_full_sort`, a proptest. It draws 0 to
  300 keys whose weights come from {0, 0.5, 1, 1.5}, so ties are frequent,
  with shuffled unique orders, and a block size from 1 to 79. It asserts that
  the heap order, fixed blocks of that size and doubling blocks from that
  size all equal `sort_unstable`. A size of 1 makes doubling start a block
  at keys 2, 4, 8, 16 and so on, so short draws reach many blocks. The spike
  holds it (`crates/index/src/query/tests.rs:236`), with the heap kept as a
  test reference until S1 drops it. S1 keeps doubling only, so the fixed
  case goes with `Blocks`.
- `query::tests::the_lazy_candidates_equal_the_sorted_ones_ties_included`
  (`:249`). It indexes six notes, one not loaded, with equal and unequal
  scores. It asserts that `candidates` and `ranked` with fixed blocks of 1,
  2 and 64 and doubling from 1 yield the same ids as the full sort, five of
  them.
- `query::tests::a_ranked_order_outlives_its_snapshot` (`:290`). It drops
  the index, then drains a `Ranked`: two candidates. With the `const`
  assertion of V4, it pins the owning form.

**Tests that failed under the spike's first commit.** Each one asserts vault
order. Every other test passed: `cargo test --workspace --no-fail-fast`.

- `inkra-index`, `search::tests::hits_come_in_vault_order_and_skip_notes_not_loaded`
  (`crates/index/src/search/tests.rs:67`). It now lists `plan plan` before
  `x plan`. The spike renames it `hits_come_best_note_first_and_skip_notes_not_loaded`
  and asserts the ranked order.
- `inkra-index`, `search::model_tests::the_two_boxes_agree` (`:191`), a
  proptest that failed on the draws.
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

With the ranked oracle and the renamed test, the spike's `cargo test -p
inkra-index` passes: 565 tests in the first revision, 566 with this
revision's test. The three app failures are reorderings of
the same rows; the spike did not rerun the app tests.

**New tests.**

- App: two notes. The later one in vault order holds the rare term more
  often, and lists first. It fails on today's code.
- Index: equal scores keep vault order. Two notes with the same text list in
  vault order.
- Index: the limit keeps the best notes. With a limit of one note's hits, the
  best note is kept, not the first one.
- Index: candidates past the first blocks. With more than 448 candidates
  and a limit that needs every one of them, the notes read are the full
  sort's, in order. Doubling from 64 starts blocks at the 65th, 193rd and
  449th key, so this covers the second and third selection and the final
  sort of the rest.
- `cargo mutants` on the build's diff, from the repo's command
  (`CLAUDE.md`, "mutants"): zero missed.

## Slices

Each slice holds data, behavior and a test.

- **S1, `inkra-index`.** `Weight` moves to `words/score.rs` (V6).
  `candidates` returns `Ranked`, best first by doubling blocks (V1, V3,
  V4). The
  oracle ranks, and the lazy-order tests land (Tests). The index tests
  change, and the new index tests land. The docs of V7 in `inkra-index`
  change. The benches collect or iterate. `cargo mutants` on the diff.
- **S2, the app.** `Reader::stream` and `stream_with` take an iterator (V4).
  The three app tests change, and the new app test lands. The docs of V7 in
  `inkra-app` and the integration spec change. A shot of the list with
  ranked groups. The cold measure passes (Cost); S2 is not done before.
  `README.md:21-22` claims "ranked by BM25" again. Roadmap item 9 is struck
  with the date.

S1 changes the type `candidates` returns, so S1 alone does not build the
app. S1 and S2 merge together.

## Deferred

| Item | Trigger |
|---|---|
| Line order inside a note: list first the lines that hold the most terms | A ranked note still shows weak lines first on the Wikipedia vault |
| Light a common term only in a line that holds a rarer one (`1,000,000` showed lines holding only `of`) | Same, after line order |
| Proximity re-ranking of the top notes, the general form (`docs/roadmap.md:306`) | Line order is not enough |
| Bound-based pruning (MaxScore, WAND): a per-term upper bound, to skip scoring notes that cannot enter the read set | The per-note floor of `candidates`, now about 0.3 to 0.5 ms at 50,000 notes, grows past 2 ms on a real vault |
| A parallel `candidates` over ranges, as `find_notes` does | Same trigger |
| Streaming results (`2026-09-24-search-design.md:1263`) | Its own trigger: first results over 100 ms |

Streaming gets harder with ranks. That row appends groups in vault order as
reads land. With ranked groups, a later read can outrank a shown group. It
would then need to insert above shown rows, or wait for the order to be
final.

## Open questions for the maintainer

1. **Does a note whose name holds the terms deserve a boost in `⌘⇧F`?**
   The spec's position: no. `⌘P` is the name search: its `Name` group comes
   before any text match (`crates/index/src/find/rank.rs:95-100`). `⌘⇧F`
   reads only the text, and the query it builds has no name context
   (`crates/app/src/vault_search/mod.rs:137`). A boost would add a second
   score that the oracle must model too.
2. **Should the header show anything about rank?** The spec's position: no.
   The order already shows it. `⌘P` shows no score either. A score number
   means nothing to a reader.

## Rejected alternatives

**Sort `Searched` after the reads.** The limit would still cut in vault
order: the reads stop at 1,000 hits (`crates/index/src/search/text.rs:113`),
so the best notes past the cut are never read. And the scores live in the
`Lookup`, which the reads do not hold.

**A separate scorer for `⌘⇧F`.** `Lookup::score` already gives the weight.
A second scorer would be a second formula that the oracle must check, and
the two searches could disagree on one note.

**A full sort of V3's keys.** It gives the same order as V4, and costs up to
1.08 ms more than `main` per keystroke (`é`, Cost). It delays the first read
by up to 1.83 ms (`e`).

**A stable sort over `(f64, Candidate)`.** The plan's proposal. It gives the
same order, and cost 2.47 ms on `candidates` alone for `e` in the first
revision, against 0.73 ms for `main`. It moves 64-byte elements and builds
every `Candidate`.

**A heap built in O(n), popped on demand.** It gives the same order. In the
first revision it was slower than blocks of 64 in 8 of 9 keystroke cases, by
up to 0.12 ms (`projet réunion`), and slower to the first read in 7 of 9.

**Fixed blocks of 64.** The first revision's V4. It ties with doubling on
the nine first cases, and costs 0.59 ms more on `kumquat`: 16 selections
over 50,000 keys against 5 (Cost). Its selections grow linearly with the
notes read.

**Fixed blocks of 256.** It ties with doubling on every measured case. Its
selections still grow linearly with the notes read: 196 at 50,000 reads
against 10 (V4, the bound).

**A borrowing iterator over the snapshot.** It saves four `Arc` increments
per query, and measured no faster than the owning `Ranked` (Cost). It would
make `Reader::stream`'s bound depend on its caller, and quick open needs a
`'static` stream (V4).

**A top-k of a fixed number of notes.** A note can give zero hits (V4), so a
fixed top-k could stop short of the limit while better notes exist. Blocks
keep the speed of a top-k without its fixed count.

**Rank inside `fan_out` over parallel ranges, as `find_notes` does.** The
lazy order already brings the keystroke to or below `main`, within the
spread of runs on `quince` (Cost). Parallel ranges
add a merge for a cost that fits the budget. Deferred, with a trigger.

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
