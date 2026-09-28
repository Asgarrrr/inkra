# Ranking key 2: weigh a term by its rarity and the note's length

Status: **implemented 2026-09-28.** R4 reads `len` at score time; see Cost.

Item 4 of the roadmap's "Index and search: the 2026-09-27 audit". It amends
key 2 of `find_notes` (`2026-09-25-smart-quick-open-design.md`, S3) for the
`NameAndText` and `Text` groups. `Name` rows keep their key 2.

## Why

Key 2 for a text match is a raw sum: the occurrences of every text-placed
term (`index/src/find/rank.rs`, `place`). Two defects follow.

1. **A common term drowns a rare one.** For `projet kubernetes`, a note with
   40 × `projet` and 1 × `kubernetes` beats one with 2 × `projet` and
   8 × `kubernetes`. The second is almost surely the one about Kubernetes:
   `projet` is in half the vault, `kubernetes` in a dozen notes.
2. **A long note wins by length.** A 20,000-word journal mentions everything
   more often than a 300-word note about the subject.

Keys before key 2 are unchanged: the group, the exact name, and key 1 (terms
matching whole words). So this only reorders rows that tie on those.

## What others do

BM25 is the default scorer of Lucene, tantivy and Meilisearch's text ranking
rule family; the audit named it. It multiplies a term's rarity (IDF) by a
saturating, length-normalized term frequency. We take it as is, with Lucene's
parameters, and no field weights: a note's name is ranked by the groups
already, never by key 2.

## Design

### R1. The score

For a note `d` and the set `T` of query terms placed in its text:

```
score(d) = Σ_{t ∈ T} idf(t) · tf(t, d) · (k1 + 1) / (tf(t, d) + k1 · (1 − b + b · len(d) / avg))
idf(t)   = ln(1 + (N − df(t) + 0.5) / (df(t) + 0.5))
k1 = 1.2, b = 0.75
```

- `tf(t, d)`: `TermHit::count`, the occurrences in `d` of every word starting
  with `t`, as today.
- `df(t)`: the notes with a nonzero hit for `t`. A term is a prefix, so this
  is the notes having any word that starts with it: the same notes the hits
  already list.
- `N`: the live notes with at least one word, `alive.len() + fresh.len()` of
  one `WordsSnapshot`.
- `len(d)`: the words in `d`'s text, repeats counted, after folding and
  capping; a word folding to nothing does not count. Same unit as `tf`.
- `avg`: the sum of `len` over the `N` notes, divided by `N`.

This IDF is never negative, unlike the classic Robertson form: a term in
every note weighs little, not less than nothing. It equals
`ln((N + 1) / (df + 0.5))`, so it stays non-negative only while `df ≤ N`.
R4 guarantees that by counting a note once per term (R4, "df").

Two pure functions compute a term's contribution, each with one fixed
expression grouping: `idf(df, n) -> f64`, once per term in the lookup, and
`bm25(idf, tf, len, avg) -> f64`, per term per candidate.
`rank::place` and the `find` model both call them, adding the terms in term
order. Two formulas that are equal on paper can differ in the last bit, and
the model proptest compares ranks exactly, so the grouping must be shared,
not rewritten. The model still computes `tf`, `df`, `N`, `len` and `avg` from
its own texts.

A `Name` row's key 2 is unchanged: the count of stem terms.

### R2. The key's type

`Rank::weight` becomes `Reverse<Weight>`, where

```rust
struct Weight(f64); // Ord by `f64::total_cmp`
```

For a `Name` row the value is the stem count as `f64`, exact up to 2^53. The
group precedes key 2, so a `Name` value never compares with a text score.
Scores are finite and non-negative by construction: every factor is.

`merge_notes` over any split still equals one range: a note's score reads
only its own hits, the lookup's `df` and the snapshot's `N` and `avg`, which
every range shares.

### R3. Storing lengths

- `NoteWords` gains `len: u32`, the sum of its counts, saturating.
- `Segment` gains `lengths: Box<[u32]>`, indexed by ordinal, filled by `merge`
  from the kept `alive` ordinals and the fresh notes. A dead or never-seen
  ordinal holds 0. This is the per-ordinal length the roadmap asks for
  before any format is persisted (item 6).
- `Words` keeps `total: u64`, the sum of `len` over its live notes. `set`
  adds the new note's `len`; `hide` subtracts the old one's, read from
  `fresh`, or from the segment when `alive` holds it. An ordinal in neither
  (an emptied note keeps its ordinal) holds no words: `hide` subtracts 0. A
  merge moves notes from `fresh` into the segment and changes no total.
- `lengths` costs 4 bytes per ordinal ever minted, for as long as the
  segment lives: 200 KB at 50,000 notes. Ordinals are never reused, but an
  autosave mints none (`Words::set` keeps an emptied note's ordinal), so only
  creating notes grows it. `Alive` has the same growth at 1 bit per ordinal.
  Accepted: compacting ordinals is a separate change with its own trigger.
- `WordsSnapshot` carries `total` with the rest; `N` it already knows.

### R4. The lookup carries what the score needs

`WordsSnapshot::lookup` already visits every hit. It also:

- counts `df` per term, when a hit's count leaves 0. `add` returns whether it
  did. A prefix term can match two words of one note (`cat` in `cats` and
  `catalog`); both land in that note's one `TermHit`, and only the first
  moves it from 0. No ordinal is both in `alive` and in `fresh`
  (`store.rs`, `Words::fresh`), so the segment and fresh loops never count a
  note twice either;
- keeps its `WordsSnapshot`, so a scored note's `len` is read at score time:
  from the segment's `lengths` while `alive` holds it, else from its fresh
  `NoteWords`;
- stores each term's `idf` and `avg`.

`Lookup::score(term, ordinal, hit)` returns one term's weight; `rank::place`
adds them in term order instead of summing counts. The lookup adds one counter
per term; no pass over the vault. (Superseded, see Cost, "As built": the draft
recorded `len` per hit ordinal during the lookup.)

## Cost

The lookup already pays one step per posting; the new work is a counter per
term and a division per term. `place` pays a length read and one `ln`-free
expression per text term per candidate: the `idf` is computed once per term in
the lookup.
Measure `find_notes` with the existing bench before and after; the budget is
no measurable regression.

**Measured 2026-09-28**, `--quick`, the Zipf corpus of 50,000 notes, settled,
the two builds run back to back:

| Bench | Before | After |
|---|---|---|
| lookup, top word | 138 µs | 143 µs |
| lookup, two words | 183 µs | 189 µs |
| lookup, 1 letter "e" | 2.45 ms | 2.52 ms |
| keystroke to names, top word | 859 µs | 892 µs |
| keystroke to names, two words | 738 µs | 764 µs |
| keystroke to names, 1 letter "e" | 3.38 ms | 3.50 ms |
| `find_notes`, one core, top word | 5.48 ms | 5.93 ms |

Text queries run 0–4 % slower end to end. Queries the change does not reach
(name only, no match, empty) moved 4–10 % either way between the same two
builds, so the difference sits at the noise floor.

**As built, R4 reads `len` at score time.** Recording it per hit posting, as
the draft of R4 said, cost 50 µs (+40 %) on a settled lookup of the top word: a store into
a growing vector per posting, on the serial keystroke path. `Lookup` keeps
its `WordsSnapshot` instead, and reads a scored note's length from the
segment's `lengths` while `alive` holds it, else from its fresh `NoteWords`.
That work moves to the parallel ranges: a bit test per text term per
candidate, and a map lookup for a fresh note. `Lookup::score(term, ordinal, hit)`
returns one term's weight, and `place` adds them in term order.
`Segment::lengths` ends at the highest live ordinal, so a segment emptied by a
merge frees it.

## Tests

- The audit's case: a vault where `projet` is in most notes and `kubernetes`
  in few. `projet kubernetes` lists the note with 2 × `projet` and 8 ×
  `kubernetes` before the one with 40 × and 1 ×. It fails on today's code.
- Length: two notes each with 3 × `graphe`, one of 30 words, one of 3,000;
  the short one ranks first. It fails on today's code.
- Saturation: 3 × vs 300 × a term, same length: 300 × still ranks first
  (BM25 is monotone in `tf`), so no test becomes flaky on a tie.
- A term matching two words of one note (`cat` in `cats catalog`) counts
  `tf = 2` and `df = 1`. A per-word `df` fails it.
- `idf` and `bm25` themselves: a hand-computed value, `bm25` zero at
  `tf = 0` and monotone in `tf`, decreasing in `df` and in `len`.
- The `find` model (`find/model_tests.rs`) computes `df`, `N`, `len` and
  `avg` from its own texts, and calls `idf` and `bm25`; the proptest keeps
  `find_notes` equal to it. The proptest gains a flag that runs
  `NoteIndex::merge_all()` before the snapshot, so segment lengths are
  checked end to end, not only fresh ones.
- `words` unit tests: `total` after set, re-set, remove, and across a merge
  that applies after writes (`Running::touched`); `Segment::merge` lengths for
  kept, dead and fresh ordinals.
- IDF alone: at equal lengths, 3 × `projet` and 1 × `kubernetes` rank after
  1 × `projet` and 2 × `kubernetes`. The audit's case also passes by
  saturation alone; this one fails with `idf` forced to 1.
- `app/src/index/corpus_tests.rs`, `a_save_ranks_its_note_first_by_mtime`:
  key 2 now depends on length, so the saved text keeps the other note's length
  and key 5, the mtime, still decides.
- `cargo mutants` on `index`: zero missed.

## Rejected alternatives

**Normalize by length alone, without IDF.** Fixes defect 2, not defect 1.

**TF-IDF without saturation.** A note repeating a word 200 times would still
dominate; BM25's `k1` exists for exactly this.

**Fixed-point `u64` score.** Keeps `Rank` integer, but picks an arbitrary
scale that either rounds real differences to ties or overflows on long
notes. `total_cmp` orders every finite `f64` exactly.
