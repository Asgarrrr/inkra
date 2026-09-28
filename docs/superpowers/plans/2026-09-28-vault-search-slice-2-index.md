# Vault search, slice 2: the index — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md`, S2.
Also the word-start spec's Budget (`2026-09-28-search-word-start-design.md`).

**Goal:** `Hit` carries the 1-based logical line of its start, and `Searched`
names the notes that gave more hits than it lists. `inkra-index` only; the app
keeps compiling with no behavior change.

**Branch:** `vault-search-2-index` from `vault-search-1-runs` (`74eb216`).

## Success criterion

Written first, failing on `74eb216` for the stated reason:

- `search/tests.rs`
  - `a_hit_knows_the_line_of_its_start`: `"a\nb a\n\n a"` gives lines 1, 2, 4.
    A note whose hit follows more than two `CANCEL_EVERY` blocks of `\n`
    gives the count plus one (the block walk).
  - `a_set_flag_stops_the_line_count`: `run::lines` with a set flag over a
    hit past byte 0 is `None`. It is the only way to reach that branch: a
    flag set before `search` returns at its top.
  - `a_note_with_exactly_100_hits_is_not_capped`, and one with 150 is:
    `capped == [ids[0]]` (extends `a_note_gives_at_most_100_hits`).
  - `a_note_cut_by_the_limit_is_capped`: `["a a a", "a a"]`, limit 4:
    `capped == [ids[1]]`; limit 3: `capped` empty (exactly `room` hits).
  - `merge_concatenates_in_range_order_and_truncates` also checks `capped`
    when the merge's cut falls inside a note, and when the note was already
    capped by its range (no duplicate).
- `search/model_tests.rs`: the model builds each `Hit` with a naive line,
  `text[..start].matches('\n').count() + 1`, and predicts `capped`: a note
  with at least one kept hit and more hits in the text than kept. `model_hits`
  loses its `HITS_PER_NOTE` truncate, so the count is the note's true one;
  the model applies the per-note cap itself. Both
  proptests (`the_two_boxes_agree` over any split, and
  `hits_equal_the_model_past_the_per_note_cap`) compare the whole `Searched`.
- `./scripts/check.sh` exits 0.

## Files

| File | Change |
|---|---|
| `crates/index/src/search/hit.rs` | `Hit.line`, `Hit::new(.., line)`, `Searched.capped`, `merge` |
| `crates/index/src/search/run.rs` | `capped` push, truncate moves out of `locate`, `lines` |
| `crates/index/src/find/terms.rs`, `find.rs` | `CANCEL_EVERY` becomes `pub(crate)`, re-exported |
| `crates/index/src/search/tests.rs`, `model_tests.rs` | the tests above |
| `docs/.../2026-09-28-search-word-start-design.md` | Budget: the late-hit figure after S2 |
| `docs/.../2026-09-28-vault-search-integration-design.md` | S2: the figure, an "As built" note |

## Data shapes

```rust
pub struct Hit {
    pub note: EntryId,
    pub range: Range<ByteOffset>,
    /// 1-based logical line of the hit's start.
    pub line: u32,
    pub context: String,
    pub context_match: Range<usize>,
}

pub struct Searched {
    pub hits: Vec<Hit>,
    pub limit_reached: bool,
    /// Notes that gave more hits than `hits` lists, in vault order.
    pub capped: Vec<EntryId>,
}
```

## Steps

1. `run.rs`: `locate` returns the walk's ranges untruncated (at most
   `room + 1`). `search` pushes `note` to `capped` when `lit.len() > room`,
   then truncates. `lit.len() > room` means the note has more hits: the
   `room + 1`-th range is distinct from the `room`-th, and a later word only
   merges into the last range.
2. `run.rs`: `pub(super) fn lines(text, lit, cancel) -> Option<Vec<u32>>`
   counts `\n` bytes between consecutive starts, in blocks of at most
   `CANCEL_EVERY` bytes, reading `cancel` before each block. `None` →
   `search` returns `Searched::default()`. Byte slices, so no char boundary
   is needed; blocks partition `from..start` exactly. A line past `u32::MAX`
   saturates: `u32::try_from(line).unwrap_or(u32::MAX)`, never `as`.
   Order in `search`: `locate` → `capped` push → `lit.truncate(room)` →
   `lines(text, &lit, cancel)` → zip 1:1 into `Hit`s.
3. `hit.rs` `merge`: concatenate hits and `capped`. Remember the note of the
   first dropped hit (`hits.get(limit)`), truncate. Keep the capped notes
   still listed with one pass: `capped` is a subsequence of the listed notes'
   order, so one shared iterator makes it O(hits):

   ```rust
   let cut = hits.get(limit).map(|hit| hit.note);
   hits.truncate(limit);
   let mut listed = hits.iter().map(|hit| hit.note);
   capped.retain(|note| listed.any(|listed| listed == *note));
   // `hits.last()` is `None` at limit 0: nothing is pushed, as one range gives.
   capped.extend(cut.filter(|cut| hits.last().is_some_and(|last| last.note == *cut)));
   capped.dedup();
   ```

   `dedup` drops the push when the note's range already capped it. Invariant comment:
   `chunks`' ranges are disjoint and contiguous, so one note's hits never
   span two ranges.
4. Bench: `cargo bench -p inkra-index --bench search -- --quick
   --save-baseline before-s2` on `74eb216`, then `--baseline before-s2`
   after. Record both in the two specs. As run: the two full runs drifted by
   60 % on every case, the no-line ones included, so the recorded figure is
   an interleaved A/B (`git stash`, twice): 49.8 ms before, 51.0 ms after.

## Verification

```
cargo test -p inkra-index
./scripts/test-names.sh   # before/after: only added names
./scripts/check.sh
cargo bench -p inkra-index --bench search -- --quick --baseline before-s2
```

`cargo mutants` is run by the orchestrator after this slice, not here.

## Red/blue on the plan

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | The model's per-note count is truncated at 100, blind to a 150-hit note | CONFIRMED | Fixed: `model_hits` returns every hit; the model caps |
| 2 | `lines` vs truncate order unstated | CONFIRMED | Fixed: order pinned in step 2 |
| 3 | `retain` + `any` is O(capped × hits) | REFUTED: one shared iterator is O(hits) | Kept; the sketch in step 3 pins the idiom |
| 4 | `merge` at limit 0 drops `capped` | Consistent: `search` at limit 0 gives none | No change; the oracle covers limit 0 |
| 5 | Saturation must not use `as` | CONFIRMED | Pinned `try_from` |
| 6 | Block edges may double-count `\n` | REFUTED: byte partition, `\n` is never a continuation byte | Test past two blocks kept |
| C | Blue: `hits[limit - 1]` underflows at 0 | Valid for an index | The sketch uses `hits.last()`, no index |

Blue also traced `each_word`: `lit.len()` never exceeds `room + 1` and never
stops at `room` while hits remain, so `lit.len() > room` is exact.

## Red/blue on the diff

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | `lines` may panic off a char boundary or loop | Red itself refuted it: `from` is always a boundary, `to > from` | No change |
| 2 | The model may be tautological | Red itself refuted it: `capped` by count, `line` by a naive count | No change |
| 3 | `merge`'s shared walk rests on the range order | CONFIRMED: already stated in comments; `EntryId` order is not vault order, so no `debug_assert` | Doc comment names the order contract |
| 4 | Spec says "`locate` stays as it is" | REFUTED after the As built note was added | As built note records the change |
| 5 | `lines` cost compounds | REFUTED: it reads a subset of the bytes `locate` read | No change |

## Follow-up: hits grouped by note

After review, `Searched` lists `NoteHits { note, hits, capped }` groups,
`Hit` loses `note`, `line` is a `usize`, and `lines` reads no `cancel` (the
integration spec's As built (slice 2) records why).

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | "Never empty" is a doc, the fields are pub | CONFIRMED; only `search` and the model build one | No change: a documented guarantee of what `search` and `merge` return |
| 2 | `search` pushes a group without checking `locate` found a hit | The index/walk agreement is the word-start spec's invariant, pinned by `the_two_boxes_agree` | No guard: it would hide a disagreement |
| 3-5 | `merge`'s `limit_reached`, `capped \|=`, the docs | Red verified them correct | No change |
| 6 | The model shares `prefix` | `word_prefix` equals `prefix` by its own proptest | No change |
