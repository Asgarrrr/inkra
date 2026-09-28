# Vault search matches word starts, as quick open does

Date: 2026-09-28. Status: **implemented**.

> **As built.** S1 to S6 hold as written, with these deviations:
>
> - A query with no term returns no hit, and `limit_reached` keeps its rule
>   (`hits.len() >= limit`), so it is true at a limit of 0 only. At any other
>   limit this is `Searched::default()`. The spec's plain default broke
>   "`merge` over any split equals one range" at a limit of 0.
> - The walk over a note stops at the first hit past its room, then drops it.
>   Until then a later word may still merge into the last range (S2), so
>   stopping at the room itself could cut that range short.
> - The "two boxes agree" proptest cuts 80 % of its queries from a loaded
>   text, 1 to 7 chars from any char. `model_cases`' own queries matched 18
>   notes in 256 cases; with the cut, 95 matched and 399 missed. A
>   deterministic test asserts at least 50 of each.
> - `search/model_tests.rs` reuses `find/model_tests.rs`' `build` and model
>   helpers, now `pub(crate)`, so both oracles draw one vault.
> - The bench merges every fresh note's words before timing, as the `find`
>   bench does. Its case ids changed, so criterion's `--baseline` cannot
>   compare them; the table under Budget pairs the figures by query.
> - The new walk case is over 16 ms: 87.6 ms, then 48.7 ms with the
>   first-byte skip. Its profile is under Budget.
> - `Terms::matching` skips any word whose first byte is ASCII and whose
>   fold starts no term, not only an all-ASCII word. `Terms` is built at the
>   first candidate, so a query no note holds does not pay for its table.

Item 3 of the roadmap's "Index and search: the 2026-09-27 audit". It amends
[`2026-09-24-search-design.md`](2026-09-24-search-design.md) (the search spec):
D4 is replaced, D7 and D9 lose the two toggles, and its Testing and Evolution
sections change as listed under "Amendments". It reuses S1, S2 and the S4
highlight rule of
[`2026-09-25-smart-quick-open-design.md`](2026-09-25-smart-quick-open-design.md)
(the quick-open spec). It must land before slice 10, the vault search delegate.

## Why

Quick open (`⌘P`) and vault search (`⌘⇧F`) answer "which notes hold these
words" with two different rules today.

- `⌘P` folds case and diacritics (S1) and matches the start of a word:
  `ete` finds `été`, `e\u{301}te` finds `été`, `jet` does not find `projet`.
- `⌘⇧F` (D4, `search/query.rs`) matches a literal substring, NFC query, case
  folded only: `ete` misses `été`, an NFC `été` misses NFD text, and `jet`
  finds `projet`.

So the two boxes disagree on whether a word is in a note's text. The user
decided on 2026-09-28: `⌘⇧F` adopts `⌘P`'s match rule. A query word matches a
text word that starts with it, after the shared fold (`words/fold.rs`, `fold`
and `word_spans`). No mid-word substring, no regex.

The invariant of this spec is on the text rule only: **for one query and one
index snapshot, `⌘⇧F` lists a note exactly when every term of the query
starts a word of the note's saved text.** It is not note-set equality with
`⌘P`. `⌘P` also matches names, and `place()` puts a term in the name first,
so a `⌘P` `Name` or `NameAndText` row may or may not carry a `⌘⇧F` hit, and a
note matched by its name alone has none. The Tests section states the exact
relation.

A second gain follows. The word index already knows which notes hold every
term, so the scan reads only those notes. Today whole word `e` scans the full
150 MB corpus in 306 to 341 ms on one core.

## What Zed does, and what others do

Zed's project search (`vendor/zed-reference/crates/project/src/search.rs`)
has two query kinds: `SearchQuery::Text`, an Aho-Corasick literal, and
`SearchQuery::Regex`. Both carry `whole_word` and `case_sensitive`
(`:76-111`). A case-insensitive non-ASCII literal falls back to an escaped
regex (`:120-124`). Zed has no diacritic folding. It runs `detect` on a file
(`:398`) before `search` over its buffer (`:509`), a per-file prefilter by
the same matcher.

Deviations, each for a note app and the rule the user chose:

- **No substring, no regex, no toggles.** A note app searches prose. The
  user wants one rule in both boxes, and each toggle would list a set of
  notes that `⌘P` cannot list.
- **The prefilter is the word index**, not a second pass of the matcher over
  each file. The index is resident and exact for this rule (S2), so a note
  it rejects is never read.
- **Diacritics fold.** Lucene's `ICUFoldingFilter` and Meilisearch's
  charabia fold them; the quick-open spec's S1 records the survey.

Full-text engines (tantivy, Lucene, Meilisearch) match whole tokens or token
prefixes from their index, then read stored text only to build snippets.
This spec does the same: the index decides the notes, the text gives the
ranges.

## Design

### S1. Query semantics

`search` takes a `NoteQuery`, built by `NoteQuery::new(text, None, &[])`. So
the query is cut, split and folded by the code `⌘P` runs, and no second
parser can drift from it.

- **Terms.** The query is split by `word_spans` and each word is folded by
  `fold`; each folded word is a term (quick-open S1). `l'été 2026-09` has the
  terms `l`, `ete`, `2026`, `09`. Non-word chars in the query are
  separators, never matched.
- **Every term, anywhere, in any order.** A note matches when each term
  starts at least one word of its saved text. Terms need not be adjacent or
  in order. This is `⌘P`'s rule for a `Text` row. A phrase rule is rejected
  (Rejected alternatives).
- **Repeats count once.** `plan plan` has one term, as in `NoteQuery::new`.
- **The 64-char cut** is `NoteQuery`'s: the first 64 typed chars, a word the
  cut falls inside is dropped unless it is the only one, and a term keeps 64
  folded chars. D4's 1,000-char cut goes.
- **Empty or blank.** A query with no term (`""`, `"   "`, `"[["`) returns
  `Searched::default()` before any lookup. The delegate shows D9's
  empty-query state for a blank field and "No matches" for a field with text
  and no term.
- **Case sensitive and whole word are removed.** A case-sensitive match
  contradicts the fold: the index holds folded words only, and a sensitive
  filter would list notes that `⌘P` does not. Whole word could survive as
  "the folded word equals the term" (`TermHit::whole` exists), but it too
  lists a set `⌘P` never lists, and `⌘P` already ranks whole words first
  (S3 key 1). Both toggles, their `Aa` and `W` trailer and the `alt-cmd-c`
  and `alt-cmd-w` keys leave D7 and D9. The delegate is not built, so no app
  code changes for this.
- **Name words do not count** (settled). `⌘⇧F` searches the saved text. A
  term found only in a note's name gives that note a `⌘P` row (`Name`,
  `NameAndText`) and no `⌘⇧F` hit, since a hit is a range of the text to
  select.

### S2. The hit

A **hit** is one text word that a term starts, lit as `⌘P` lights it.

- Its range is `highlight::prefix`'s rule: from the word's first grapheme to
  the first grapheme end whose run folds to a start of the term. So `ete`
  over `Été` lights `Été`, `ét` over `été` lights `ét`, and a range is
  always whole graphemes of the saved text. The editor selects this range on
  `HitActivated` (D10).
- A word that two terms start (`pro` and `proj` in `projet`) gives one hit,
  for the longer term. Two terms that start one folded word are prefixes of
  each other, so the longer one's range holds the shorter one's.
- Ranges in one note are ascending and disjoint: a range that touches or
  overlaps the previous one merges into it, as `highlight::light` merges.
  This happens only when a grapheme spans two words (`a🏻\u{301}b`).
- `Hit` keeps its fields. `range` is a `ByteOffset` range into the saved
  text. `context` and `context_match` keep D4's rule: the hit widened within
  its line by up to 60 bytes before and 100 after, on char boundaries
  (`window::around`).

### S3. Candidate notes

`search` reads one `IndexSnapshot` and one `Lookup`, as `find_notes` does:

```rust
pub fn search(
    snapshot: &IndexSnapshot,
    lookup: &Lookup,         // snapshot.lookup(query, cancel), once per keystroke
    range: Range<usize>,     // from snapshot.chunks(n)
    query: &NoteQuery,
    cancel: &AtomicBool,
    limit: usize,
) -> Searched;
```

For each loaded note of `range`, in vault order, `search` resolves the note's
ordinal once (`Lookup::ordinal`). It reads the note's text only when
`Lookup::hit` is `Some` for every term. A note with no ordinal has no words
and is skipped.

**Why the prefilter is exact.** `NoteWords::parse` builds a note's words from
its text with `word_spans`, `fold` and a 65-char cap. The scan in S4 runs
`word_spans` and `fold` over the same text. A term has at most 64 folded
chars, so a folded word starts with it exactly when its capped form does.
So the index holds a term for a note exactly when the scan finds a word the
term starts. Every candidate gives at least one hit, and no note outside the
candidates could give one.

**Why words and text are of one version.** `NoteIndex::insert` writes the
corpus slot and the words from one `NoteContent`, parsed from one
`Arc<str>`, in one `&mut self` call. `remove` and `set_notes` drop both
together. `NoteIndex::snapshot` clones the names, corpus and words in one
`&self` call. So inside one `IndexSnapshot`, a note's words are
`NoteWords::parse` of its corpus text. `search` takes the `IndexSnapshot`,
not a `CorpusSnapshot`, so a caller cannot pair a corpus with the words of
another moment. The `Lookup` must come from the same snapshot, the contract
`find_notes` already states. A merge (`words_apply`) swaps a segment for an
equal one and changes no match.

### S4. Locating occurrences

For each candidate, `search` walks its words with `find/terms.rs`'s
`each_word` (in `find/excerpt.rs` before this change), which reads `cancel` every 64 KB and joins a word that crosses a
block edge. Each word goes through `Terms::matching`: an ASCII word whose
first byte starts no term is skipped unfolded, else the word is folded once
and compared with every term. The longest matching term gives the hit range
through S2.

`prefix` needs the grapheme bounds around the word, and `boundaries(text)`
over a whole 10 MB note per query is too costly. A new
`highlight::word_prefix(text, word, term) -> Range<usize>` finds the same
range with `unicode_segmentation::GraphemeCursor` over the whole text: the
grapheme start at or before the word start, then grapheme ends one at a time
until the run folds to a start of the term or reaches the word end. Its cost
is the hit's graphemes. A proptest keeps it equal to
`prefix(text, &boundaries(text), word, term)`.

The walk over a note stops at `HITS_PER_NOTE` hits or at `limit` in total.

### S5. Order, limits, cancel

- **Order.** Notes in vault order, hits in text order, as D4. `⌘P` ranks
  and `⌘⇧F` lists; the user's rule is about which notes match, not their
  order. Settled: no BM25 note order in this change. It stays the search
  spec's deferred item and the "Ranked notes" row of Evolution,
  "Relevance ranking of content hits". The `Lookup` already holds each
  candidate's score (`Lookup::score`), so that change reads no text.
- **Limits.** `HITS_PER_NOTE` (100) and 1,000 in total, unchanged.
  `limit_reached` and `Searched::merge` are unchanged, so any split of the
  ranges still equals one range.
- **Cancel.** `IndexSnapshot::lookup` reads it per term. `search` reads it
  before the first note, between notes, and every 64 KB inside a note
  (`each_word`). A set flag returns `Searched::default()`. Today a 10 MB
  note is one uncancellable `find_in`.
- **No per-note byte cap.** A cap, like the excerpt's 1 MiB, could drop a
  real match past it and break the invariant. So the walk over one
  candidate is O(its bytes) in the worst case: a rare term once at the end
  of a 10 MB note reads all 10 MB. The walk runs off the foreground, and the
  64 KB cancel reads bound how long a superseded query keeps a core.

### S6. What goes, what moves

- `search/query.rs` (`TextQuery`, `find_in`, the whole-word edge check) is
  deleted, and `TextQuery` leaves `lib.rs`. `⌘F` in-note find is deferred
  and has no spec. When it comes, its spec decides its rule; a literal
  substring there would make a third box disagree with two, so keeping
  `find_in` for it would presume that decision. Git keeps the code.
- `regex` moves to `[dev-dependencies]` of `inkra-index`: only the fold
  tests use it after the change. `regex-syntax` stays (`fold.rs`).
- `find/excerpt.rs`'s `Terms` and `each_word` move to a new
  `find/terms.rs`, `pub(crate)`, now that two modules call them.
  `highlight::prefix`, `light` and the new `word_prefix` become `pub(crate)`.
  `search` imports them from `crate::find`. No logic changes in the move.
- `search/run.rs` holds the new `search`; `search/hit.rs` is unchanged.
- `crates/app/src/index/tests.rs`: `found` builds a `NoteQuery` and runs
  `search` over `links.index().snapshot()`, so the `IndexSync` tests now
  check the corpus and the words end to end. The model snapshot used the
  query `[[` to read every note's whole text, which word starts cannot do.
  `CorpusSnapshot::text(order)` becomes `#[doc(hidden)] pub`, as
  `WordsSnapshot::ordinal_span` already is in this crate for tests and the
  bench, and the snapshot reads each note's text through it.

## Budget

The requirement stays: results within 100 ms of the debounce's end at
50,000 notes (search spec, Requirements). The target of this spec is tighter:
**one keystroke, the lookup plus `search` over all ranges on one core, under
16 ms for every case of the `search` bench.** Today whole word `e` takes 306
to 341 ms, `projetzz` 115 ms, and a rare word 20 ms on one core.

Why it should hold: a query with no match costs the lookup only (`find`
bench: 0.14 ms for a top word, 2.5 ms for `e`). A query with matches reads
at most the first 1,000 candidates of each range, since each gives at least
one hit (S3), and most stop at the first `HITS_PER_NOTE`. At 3 KB a note,
1,000 notes are 3 MB of word walking.

Measure with `cargo bench -p inkra-index --bench search`, saving a baseline
on today's code first. The bench's `TextQuery` cases become `NoteQuery`
cases over the same generated corpus, with the lookup inside the timed
closure: `zanzibar`, `projet`, `e`, `projetzz`, `ete`, `é`, `réunion`,
`projet réunion`, `e e`. The `long lines` and `scripts` groups keep their
notes and needles. Record each figure beside the old one in this spec. A
case over 16 ms is recorded with its profile, not hidden.

A **new case** isolates the per-note walk: one rare term that occurs exactly
once, near the end of one note of about 10 MB of prose, so the word index
lists that note and the walk reads nearly all of it. No existing case
covers this: `long_notes` never inserts `zanzibar`, so under the prefilter
it becomes a free miss, and the `scripts` needles hit early and stop at
`HITS_PER_NOTE`. Its figure is recorded here, with its profile if it passes
16 ms, per the rule above.

The `find` bench's lookup cases cover the lookup; they do not change.

### Measured

`cargo bench -p inkra-index --bench search -- --quick`, Apple M5 Pro, one
core, median. "Before" is `search` alone with `TextQuery` (baseline
`before-word-start`); "after" is the lookup plus `search` (baseline
`after-word-start`). A "whole word" before figure is the old whole-word
case of the same needle.

| Case | Before | After |
|---|---|---|
| 50k, `zanzibar` (50 hits) | 19.93 ms | 2.16 ms |
| 50k, `projet` (1,000 hits) | 0.166 ms | 1.43 ms |
| 50k, `e` (1,000 hits) | 0.078 ms | 1.08 ms |
| 50k, `projetzz` (no hit) | 114.5 ms | 0.62 ms |
| 50k, `ete` (1,000 hits) | — | 2.52 ms |
| 50k, `é` (1,000 hits) | 258.0 ms, whole word | 1.07 ms |
| 50k, `réunion` (1,000 hits) | 0.330 ms, whole word | 2.79 ms |
| 50k, `projet réunion` (1,000 hits) | — | 1.42 ms |
| 50k, `e e` (1,000 hits) | 28.6 ms, whole word, no hit | 1.08 ms |
| 10 MB, lines of 80, `projet` | 14.0 µs | 100.8 µs |
| 10 MB, one line, `projet` | 14.9 µs | 102.3 µs |
| 10 × 1 MB, lines of 80, `projet` | 143 µs | 1.02 ms |
| 10 × 1 MB, one line, `projet` | 149 µs | 1.01 ms |
| each `long lines` case, `zanzibar` | 1.26 ms | 0.30 to 0.39 µs |
| **new**: 10 MB, lines of 80, `zanzibar` once at its end | — | **87.6 ms**, 48.7 ms with the skip |
| same, with `Hit.line` (integration spec S2) | S2 before: 49.8 ms | S2 after: 51.0 ms |
| cyrillic `е`, `ет`, `зzz` (no hit) | 20.4, 11.4, 1.26 ms | 0.30 µs each |
| cyrillic `о` | 23.1 ms, no hit | 267 µs, 100 hits |
| greek `σ`, `α`, `ο` | 32.4, 22.3, 20.2 ms, no hit | 170, 440, 351 µs, 100 hits |
| cjk `的` | 4.72 ms | 0.30 µs, no hit |
| cjk `项`, `会` | 6.52, 6.41 ms, no hit | 132, 104 µs, 100 hits |

Every 50,000-note case is under 3 ms, against 16. The cases that got slower
(`projet`, `e`, `réunion` over 50,000 notes, `projet` over long notes) found
their hits in the first notes before; now they pay the lookup and a fold per
candidate word, and stay under 3 ms. A `cjk` word is its whole run of
ideographs, so `的` starts none: it was a substring match before.

The `Hit.line` row is an A/B of the integration spec's slice 2, before and
after, interleaved twice: the line count adds about 1.2 ms over 10 MB.

**The new case is over budget: 87.6 ms, then 48.7 ms.** First profile
(Time Profiler, samples inside `search`): `fold` 50 % inclusive,
`word_spans` 34 %. A quarter of the prose words are accented, and
`Terms::matching` folded every non-ASCII word. It now skips any word whose
first byte is ASCII when that char's fold starts no term. This is exact:
`fold` lowercases an ASCII char to one ASCII char, NFD never moves a starter,
and the marks it drops or keeps all follow the first char. A proptest checks
the skip against no skip. Every other case stayed within noise or got
faster. Second profile: `word_spans` 57 %, `fold` 27 % (words starting
with `é`), the walk's closure 12 %. `word_spans` alone costs about 27 ms
over 10 MB. A byte-level `word_spans` with an ASCII fast path gained only
5 %, and was not kept. Under 16 ms needs a different walk, for example a
scan for the chars whose fold can start a term, before any word split. The
walk is off the foreground and cancels every 64 KB, so a superseded query
probably frees its core within about 0.3 ms.

## Tests

Each test below fails on today's code for the stated reason, not only
because the API changed.

- **Two boxes agree** (`search/model_tests.rs`, proptest over
  `find/model_cases.rs`' vaults, loads and queries, with the merge flag of
  the ranking spec). For one `IndexSnapshot` and one query, with limits
  above the case's size:
  - the notes with a `search` hit equal the notes whose every term starts a
    word of the saved text, by a naive model (`word_spans`, `fold` and
    `starts_with` over each text);
  - every `Text` row of `find_notes` has a hit, and every note with a hit is
    a `find_notes` row.
  Today: an accented or NFD case lists different notes.
- **Hits equal the model's.** Per note, the hit ranges equal the model's:
  each word some term starts, lit by `prefix` over `boundaries(text)`, for
  the longest term, touching ranges merged, capped at `HITS_PER_NOTE`.
- **Folding.** `ete` finds `été`, `Été` and `e\u{301}te`; an NFC `été` finds
  NFD `e\u{301}te` and the reverse; `οδυσσευς` finds `ΟΔΥΣΣΕΥΣ`.
- **Word start only.** `jet` finds nothing in `projet`; `proj` finds
  `projet`. Today `jet` hits.
- **Any order.** `réunion projet` finds a note holding `projet … réunion`
  on two lines. Today the literal phrase misses.
- **Punctuation-only query.** `[[` returns no hit and no lookup runs. Today
  it hits every link.
- **Graphemes.** `word_prefix` equals `prefix` over whole-text bounds
  (proptest over `test_support::PARTS`, which holds U+1F3FB and U+0600). A
  word starting inside a grapheme (`🎉\u{301}x`) lights from the grapheme
  start; a word ending inside one (`a🏻`) lights through its end.
- **One hit per word.** `pro proj` over `projet` gives one hit, `proj`'s.
- **Limits.** 150 matching words in one note give 100 hits; 1,000 in total
  set `limit_reached`; `merge` over any split equals one range. Kept from
  today's tests with word queries.
- **Cancel.** A flag set before the call returns nothing, and so does a flag
  set before `lookup`. The 64 KB reads inside a note are `each_word`'s,
  already tested with the excerpt; no test times a mid-note stop.
- **One version.** After a save that changes a note's text, the snapshot
  taken after it hits the new words and not the old ones; a snapshot taken
  before it still hits the old ones.
- **App.** `IndexSync` tests pass through the new `found`; a load, a save, a
  rename and a trash still show in `search`.
- `cargo mutants` on `search/` and `find/terms.rs`: zero missed.

The hit oracle reuses `highlight::prefix` and `boundaries` for its ranges, so
it is not independent there. They are pinned by the `word_prefix == prefix`
proptest and by hand cases in `highlight_tests.rs`.

Removed with `TextQuery`: the v1 regex differential oracle, the whole-word
and case-sensitive tests, and "an NFC query over NFD text finds nothing".

## Amendments to the search spec

- **D4** is replaced by S1 to S6 here.
- **D7, D9**: no toggles, no `alt-cmd-c` or `alt-cmd-w`.
- **Testing**: the "differential oracle" contract becomes "Two boxes agree".
- **Evolution**: the "Bigram prefilter", "Whole-word check in Rust" and
  "`memchr::memmem` fast path" rows are superseded by the word-index
  prefilter. "No resident text" becomes: the word index lists the
  candidates, and only they are read.
- **Roadmap**: when this ships, `docs/roadmap.md` item 3 of the 2026-09-27
  audit is struck as done and points at this spec.
- **Deferred**: "Accent folding and NFD in note text" is done. "Regex
  queries" needs its own spec: a regex is not a word start, so it would
  break the invariant.

## Evolution

| Lever | Trigger | What changes |
|---|---|---|
| Ranked notes | The user finds vault order unhelpful | Candidates in BM25 order from the `Lookup`, then hits in text order; the merge sorts by score instead of range order |
| Phrase queries (`"…"`) | The user asks | Word positions in the index, or a phrase check over the candidates' text; a quoted phrase narrows the `⌘P` set, so both boxes take it |
| Whole-word toggle | The user asks | Filter by `TermHit::whole` in both boxes, never in one |
| Faster walk over a huge note | A user-visible slow `⌘⇧F` on a huge note. The 10 MB walk case takes 48.7 ms against 16 (Budget) | Scan for the chars whose fold can start a term before splitting words, so `word_spans` runs only around them |

For the slice 10 delegate: a blank query must run no lookup. `search` cannot
test that, since the caller takes the lookup, so the delegate's tests do.

## Rejected alternatives

**A fold-insensitive regex.** Compile each term to a regex whose chars match
every spelling that folds to them. The class per char is large (every
diacritic, every case orbit, NFD sequences), the regex leaves the lazy DFA,
and a regex engine and S1's `fold` would be two definitions of one rule. The
two boxes would agree by test, not by construction.

**A folded corpus copy with an offset map.** Keep a folded text per note
and a map from folded bytes back to saved bytes. It doubles the resident
text (about 150 MB more at 50,000 notes) and adds a map per note, to answer
a question the word index already answers. A substring search over folded
text also keeps mid-word matches, which the user rejected.

**Keep `TextQuery` and prefilter by the index.** The roadmap's second
option: verify candidates with `find_in`. `find_in` matches substrings and
does not fold diacritics, so it finds a different set from the one the index
proposes. The boxes would still disagree.

**A phrase rule.** Terms adjacent and in order. The word index stores no
positions, so a candidate could hold every term and no phrase, and the scan
would read notes that give nothing. It also lists fewer notes than `⌘P`,
which breaks the invariant.
