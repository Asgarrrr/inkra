# Search — design

Date: 2026-09-24. Status: **reviewed**, before code.

The application is "a vault of markdown files, links and backlinks, search,
panes". Links shipped (`2026-09-24-links-and-backlinks-design.md`), and that
spec reserved full-text search for `inkra-index`, fed by the same loads as
links (its D6). This spec adds two features:

- **Quick open** (`⌘P`): a fuzzy match on note names and paths.
- **Vault search** (`⌘⇧F`): a literal full-text search across every note.

Both open the same modal picker. Both rest on `inkra-index`.

A first draft was reviewed by three adversarial passes (correctness against
the code and gpui, performance at 50,000 notes, external facts) and two
constructive passes (evolvability, simplicity and UX). Each adversarial
finding was checked by an independent verifier; none was refuted. Where the
draft was wrong, a **Corrected** note records it.

## Requirements

- **Scale.** A vault of 50,000 notes stays usable.
  - Quick open answers within one frame, 16 ms, from the keystroke.
  - Vault search shows results within 100 ms of the end of the debounce.
  - No step runs over 8 ms on the foreground at that scale.
  - The benches below measure each figure in release.
- **Evolvable.** Performance, memory and speed will be improved after v1. The
  engine sits behind a narrow API (D1), so its storage and matching can change
  without touching `app`. Every later change passes the same oracles and is
  measured by the same benches against a saved baseline. The levers and their
  triggers are listed under "Evolution".
- **What was saved.** Vault search reads the last saved text, as the link
  index does (links spec, D5). Autosave runs about two seconds after typing.

## Scope

| In | Out, deferred below |
|---|---|
| Quick open: fuzzy match on name and path, accent-insensitive, in-memory MRU | Persisted frecency, typo tolerance |
| Vault search: literal text, case and whole-word toggles | Regex, `path:`/`tag:` operators, accent folding in note text |
| One modal picker for both, hits grouped under a header per note | Find in the current note (`⌘F`), replace |
| Opening a search hit at the match | Search in unsaved buffers, streaming results |
| Benches at 50,000 notes and an oracle for each matcher | A persisted index |

## What Zed does, and what we take from it

Paths are under `vendor/zed-reference/crates/`. Two Zed features are the
reference: the file finder for quick open, and the Text Finder
(`search/src/text_finder/`), a modal picker over project search, for vault
search.

Taken:

- **A picker split from its data.** `Picker<D>` owns the query field and the
  list. `PickerDelegate` owns the matches and renders one row
  (`picker/src/picker.rs:127-153`, `:164-383`). We have two delegates, quick
  open and vault search, so the trait has two real implementers.
- **A modal layer that occludes the views below.** A full-window backdrop
  with `.occlude()` dismisses on click. The inner container, also occluded,
  stops propagation (`workspace/src/modal_layer.rs:299-327`). The layer stores
  the focused handle when a modal opens and restores it on dismiss
  (`:160`, `:254-257`). It dismisses on focus-out (`:189-193`).
- **Cancellation in three layers** (`file_finder/src/file_finder.rs:1069-1105`,
  `picker/src/picker.rs:1244-1250`). Replacing the pending `Task` drops it. A
  shared `AtomicBool` stops the matcher mid-run. A monotonic search id drops a
  result that arrives after a newer query.
- **A name bonus.** A match in the file name outranks a match in the folder
  (`fuzzy_nucleo/src/paths.rs:113-135`, `:194-197`). Terms separated by spaces
  match in any order (`fuzzy_nucleo/src/fuzzy_nucleo.rs:51-87`).
- **Accent-insensitive names, one way.** Zed builds its pattern with nucleo's
  `Normalization::Smart` (`fuzzy_nucleo/src/fuzzy_nucleo.rs:73-78`): `ete`
  finds `été`, `été` does not find `ete`.
- **A character-set prefilter.** Each path's `CharBag` is computed once, at
  scan time. A candidate that lacks a query character is rejected before any
  scoring (`fuzzy/src/char_bag.rs:8-31`, `worktree/src/worktree.rs:6521-6525`).
- **Parallel segments on the gpui executor**, counted by
  `BackgroundExecutor::num_cpus()`, which returns 4 under test
  (`gpui/src/executor.rs:261-267`, `fuzzy_nucleo/src/paths.rs:290-354`).
- **Hits grouped under a non-selectable header per file** (Text Finder,
  `text_finder/delegate.rs:113-117`, `:822`).
- **Case and whole-word toggles** in the search field's trailer
  (`text_finder/delegate.rs:733-775`).
- **Caps with a "limit reached" signal** (`project/src/project_search.rs:165-166`,
  `:635-662`).
- **A stale finder re-runs its query** when the worktree changes, after 200 ms
  (`file_finder.rs:843-844`, `:1003-1016`).
- **An MRU of about 20 notes** for an empty query, deduplicated from the
  results (`file_finder.rs:557-613`, `workspace/src/workspace.rs:178`).
- **A placeholder and a "no matches" text** per picker
  (`picker/src/picker.rs:222-225`).
- **Startup loads bounded by the core count, at low priority** (slice 5).
  The `BackgroundScanner` runs `num_cpus()` workers under
  `scoped_priority(Priority::Low, …)` (`worktree/src/worktree.rs:5289-5343`),
  so at most one scan job per core runs at once, and user work goes first.
  `IndexSync` keeps at most one load per core in flight, each spawned at
  `Priority::Low`.
- **Take everything already landed in one go.** Zed drains every ready batch
  of file events with `poll!` before it processes them
  (`worktree/src/worktree.rs:4544-4547`). `IndexSync` applies every landed
  load in one foreground turn, up to its time budget.

Deviations:

- **No rereading of every file on every query.** Zed rereads the worktree on
  each project search (`project_search.rs:444-529`). At 50,000 notes of 3 KB,
  open and read took 1.1 to 1.25 s per query, on 10 threads with a warm cache
  (measured in review). We keep the saved text in memory instead (D3).
- **No open-buffer precedence** (`project_search.rs:914-925`), for the reason
  the links spec gives in D5.
- **Whole word decided per char, not per byte.** Zed tests the first and last
  byte of the escaped pattern (`project/src/search.rs:243-254`), so an edge
  like `é` gets no `\b`, and whole word does nothing. We test chars (D4).
- **Debounce only while a search runs**, for vault search too. The Text Finder
  always waits 100 ms (`text_finder/delegate.rs:716-719`, `:891`); the file
  finder waits only when a search is in flight (`file_finder.rs:1944-1954`).
  We use the file finder's rule for both, since an idle picker then answers
  the first keystroke at once.
- **Results arrive once, not streamed.** The Text Finder appends batches of
  256 (`text_finder/delegate.rs:1227-1315`). Our per-range scans fit the
  budget (Performance); streaming is a lever.
- **Accessibility roles on the picker.** Zed's picker sets none. We set them
  (D8).
- **Startup results applied in timed turns, not in 100 ms snapshots**
  (slice 5). Zed's workers write into one background snapshot under a mutex.
  One worker sends it to the foreground at most every 100 ms
  (`FS_WATCH_LATENCY`, `worktree/src/worktree.rs:83`, `:5310-5329`), and the
  foreground swaps it in whole (`:1407-1426`). Our index lives on the
  foreground and has no executor (D1), so each load crosses to it and is
  applied note by note. A per-note apply has a cost, so a turn stops after
  4 ms and yields. A load applies as soon as it lands, so search sees the
  note at once; readers throttle their own refresh (`BUILDING_REFRESH`).
- **Jobs are files from a known list, not directories found on the way.**
  Zed's scan jobs discover subdirectories and queue them (`:5380-5670`). Our
  vault is already scanned when loads start, so a job is one note, and the
  queue keeps D5's sequence rule.
- **The in-flight count ends at the apply, not at the read.** A load counts
  until the foreground applies it, so at most one load per core waits in
  memory. Zed bounds workers, not results, since its results share one
  snapshot.
- **Our own matcher, not a port.** `fuzzy`, `fuzzy_nucleo` and `picker` are
  GPL-3.0-or-later (`fuzzy/Cargo.toml:6`). We take the ideas above and write
  our own code. `nucleo-matcher`, which Zed uses, is MPL-2.0 and allowed by
  `deny.toml`, but its last release is from 2024-02-20, which fails the
  twelve-month rule in `rules/dependencies.md`.

## What fff does, and what we take from it

fff (`github.com/dmtrKovalenko/fff`, read at `e3f694a`) is a Rust file finder
with content search. It targets code repositories up to Chromium size.

Taken:

- **A current-note penalty.** The note already showing is not the best answer
  to "open a note". fff subtracts a quarter of its match score
  (`crates/fff-core/src/score.rs:790`, `:983-986`).
- **Results in file order, capped**, not ranked by relevance
  (`grep/grep.rs:664-707`). Ranking content hits is deferred here too.
- **A cancel check every few files** during a content search
  (`grep/grep.rs:571-660`).
- **A bigram prefilter**, as the first lever and not v1
  (`index/bigram_filter.rs`). fff reports about 30 MB for 50,000 files.

> **Corrected.** The draft cited fff for "content held in memory". fff does
> the opposite for note-sized files. It keeps only its bigram index resident,
> mmaps files of 16 KB or more within a 512 MB budget, and rereads smaller
> files on each grep (`types.rs:258-262`, `:725-727`, `:811-816`;
> `constants.rs:12-17`). fff is evidence for the "no resident text" lever, and
> that lever needs the prefilter first.

Rejected:

- **fff as a dependency.** `fff-search` 0.11 pulls 203 crates, vendored
  libgit2 and LMDB, and 123 `unsafe` sites.
- **Git boosts, LMDB, the SIMD path arena, mmap caching.** They are code
  repository concerns, or they solve a scale that a note vault does not have.
- **Persisted frecency in v1.** The decay formula (`dbs/frecency.rs:373-418`)
  is recorded in Deferred, with its trigger.

## Decisions

### D1. One owner in `inkra-index`, pure functions, no executor

`LinkIndex` is renamed `NoteIndex`, in its own commit, since it no longer
indexes only links. It owns everything keyed by note membership:

| Field | Role |
|---|---|
| `notes: NoteTable` | membership, vault order, link resolution (unchanged) |
| link sources and `by_stem` | backlinks (unchanged) |
| `names: Names` | quick-open candidates (D2) |
| `corpus: Corpus` | saved text (D3) |
| `words: Words` | the word index for quick open (smart quick open spec, S2; slice 7) |

Three calls change all four together, with one membership check and one
pruning rule:

```rust
impl NoteIndex {
    pub fn set_notes(&mut self, notes: NoteSet) -> NoteSet;   // swap, prune, reorder corpus; returns the previous set
    pub fn insert(&mut self, id: EntryId, content: NoteContent); // ignored for a non-note (slice 7)
    pub fn remove(&mut self, id: EntryId);
    pub fn names(&self) -> NamesSnapshot;                    // Arc clone
    pub fn corpus(&self) -> CorpusSnapshot;                  // Arc clone
    pub fn words(&self) -> WordsSnapshot;                    // Arc clones (slice 7)
    pub fn words_merge(&mut self, when: MergeWhen) -> Option<WordsMerge>; // run off the foreground (slice 7)
    pub fn words_apply(&mut self, merged: MergedWords) -> RetiredWords;  // free the result off the foreground (slice 7)
}
impl NoteSet {
    pub fn build(entries: Vec<(EntryId, String, Kind)>) -> NoteSet; // NoteTable + Names, off the foreground
}
```

> **Corrected in slice 7.** `insert` takes a `NoteContent`, parsed off the
> foreground (links and words), in place of a `Source` and a text. The three
> calls now change five fields together: `words` joins them, and a merge of the
> word index is started and applied through `words_merge` and `words_apply`.

> **Corrected in slice 8.** `NoteSet::build` takes `(EntryId, String, Kind,
> Mtime)`: each corpus slot holds its text and that text's mtime, and
> `NoteContent::parse` takes the mtime. Quick open's matcher is `find_notes`,
> which reads the names, the corpus (for the mtimes) and a `Lookup` (smart
> quick open spec, S3, "Built in slice 8").

A load that finishes after its note was trashed is ignored by `insert`, for
links, text and words alike (`crates/index/src/index.rs:44-49`).

> **Corrected.** The draft had `Corpus` and `Names` fed beside `LinkIndex`.
> A late load after a trash would then put the text back, since only
> `NoteTable` knows membership, and it is `pub(crate)`
> (`crates/index/src/notes.rs:21`).

The matchers are pure functions over immutable snapshots:

```rust
// names
pub struct NameQuery;           // NameQuery::new(text, showing: Option<EntryId>, recent: &[EntryId])
pub struct NameMatch { pub id: EntryId, pub score: i32, pub positions: Vec<usize> }
impl NamesSnapshot { pub fn len(&self) -> usize; pub fn chunks(&self, n: usize) -> Vec<Range<usize>>; }
pub fn match_names(names: &NamesSnapshot, range: Range<usize>, query: &NameQuery,
                   cancel: &AtomicBool, limit: usize) -> Vec<NameMatch>;
pub fn merge_names(chunks: Vec<Vec<NameMatch>>, limit: usize) -> Vec<NameMatch>;

// search
pub struct TextQuery;           // TextQuery::new(text, case_sensitive, whole_word) -> Option<TextQuery>
impl TextQuery { pub fn find_in(&self, text: &str, limit: usize) -> Vec<Range<ByteOffset>>; }
pub struct Hit { pub note: EntryId, pub range: Range<ByteOffset>,
                 pub context: String, pub context_match: Range<usize> }
pub struct Searched { pub hits: Vec<Hit>, pub limit_reached: bool }
impl CorpusSnapshot { pub fn chunks(&self, n: usize) -> Vec<Range<usize>>; }
pub fn search(corpus: &CorpusSnapshot, range: Range<usize>, query: &TextQuery,
              cancel: &AtomicBool, limit: usize) -> Searched;
impl Searched { pub fn merge(chunks: Vec<Searched>, limit: usize) -> Searched; }
```

`inkra-index` has no executor and reads no clock. `app` asks a snapshot for
`chunks(n)`, runs each range on gpui's background executor, and calls the
merge. The executor gives cancellation and deterministic tests.

> **Corrected.** The draft cited a "no rayon" rule in `docs/tooling.md`. There
> is none; that file only drops criterion's default `rayon` feature. The
> choice above stands on its own reason.

**This is the seam for evolution.** `app` sees queries, snapshots, opaque
ranges, merges and owned results. It never sees how text is stored, how
chunks are balanced, or how a candidate is rejected. `Hit` carries its own
excerpt, as `SourceLink` does (`crates/index/src/source.rs:19-25`), so no row
reads the corpus. `find_in` is public because two deferred features need it
on a single text (Find in note, search in unsaved buffers), and because the
"no resident text" lever runs it in `app` over loaded candidates.

### D2. The name matcher

> **Amended 2026-09-25.** Quick open no longer uses this matcher: see
> [`2026-09-25-smart-quick-open-design.md`](2026-09-25-smart-quick-open-design.md)
> (word-start matching on names and text, a word index). `match_names` stays
> for `[[` completion. The slice list below is renumbered there.

Candidates are notes, never folders. A candidate is the note's path as the
vault stores it, NFC, `/`-separated, `.md` stripped. Its stem is the last
component. `NoteSet::build` computes each candidate's folded chars and
`CharBag` once.

The query is NFC-normalized, then split on whitespace into terms. A note
matches when every term is a subsequence of its path, in any term order.
Chars compare by this rule, which the proptest oracle shares:

- Case folds per `char` with `char::to_lowercase`; a char whose lowercase is
  several chars compares by the first.
- Accents fold one way, as nucleo's `Smart`: a query char with no diacritic
  matches a path char whose base is that char. The base is the first char of
  the path char's NFD decomposition, from `unicode-normalization`, already a
  dependency. `ete` finds `été`; `été` does not find `ete`.

Positions index the path's chars as stored, so folding needs no offset map.

For each term, a dynamic program finds the best alignment. Scores are
integers, so ties are exact and the order is deterministic:

| Signal | Effect |
|---|---|
| A match at the start of the stem | bonus |
| A match after `/`, `-`, `_`, space, `.`, or at a lower→upper case change | bonus |
| A match right after the previous match | bonus |
| A gap between matches | penalty that grows with the gap, with a floor |
| Every matched char of a term inside the stem | bonus |
| The stem equals the query's terms joined by one space, folded | large bonus |
| The note currently showing (`NameQuery::showing`) | minus a quarter of its score, as fff does |

Ties break by MRU rank (`NameQuery::recent`), then shorter path, then vault
order. `match_names` applies the whole order and keeps a bounded top-`limit`
heap. It computes `positions` only for the matches it keeps. `merge_names`
sorts the chunks' matches by the same order and truncates. So splitting never
changes the result.

> **Corrected.** The draft applied the MRU tie-break and the current-note
> penalty in `app`, after each range had already cut to 100. The right note
> could be lost at the cut.

An empty query lists the MRU (D6) without the note showing. A query with
results lists every match, best first, capped at 100 like Zed
(`file_finder.rs:609`). From slice 3, `app` runs
`match_names` over `chunks(num_cpus)` and merges; see "Parallel quick open"
in Evolution.

**Settled in the plan.** The slice 1 plan and its review fix these points D2
left open:

- `NameMatch` carries `path: Arc<str>`, the NFC path its positions index. A
  row draws that string, not the vault's path, which is NFD on macOS.
- Positions are byte offsets into `NameMatch::path`, on char starts.
  `StyledText::with_highlights` takes byte ranges.
- No mutable `Names` type. `NoteSet` holds a `NamesSnapshot`; `names()`
  clones it.
- `match_names` handles the empty query: the recent notes in the range,
  without `showing`, by MRU rank. A recent id no longer in the vault is not
  found, so `app` does no filtering (D6).
- `NoteSet::names()` is public, so tests and the bench reach names before
  `NoteIndex` exists. Slice 2 keeps it.
- Score constants: match 16; boundary 24; stem start 32; consecutive 16; a gap
  of `g` chars costs `min(3 + g − 1, 12)`; 8 per char of a term wholly in the
  stem; exact stem 1024; the note showing loses `score / 4`. No leading-gap
  penalty.
- The exact-stem bonus compares the stem with the query's terms joined by one
  space, by the char rule. `meeting notes` gains it on `x/meeting notes`. The
  terms are taken as typed, before repeats are dropped, so `plan plan` gains
  it on the stem `plan plan`.
- A term repeated after folding counts once. `plan plan` scores as `plan`.
- The query keeps its first 64 chars after NFC (`QUERY_MAX_CHARS`). A match
  costs terms × term chars × path chars, and dropping repeats does not bound
  a paste of many distinct terms.
- `NamesSnapshot` and `NameQuery` have `is_empty`.
- "Shorter path" in the tie-break counts chars, not bytes.

### D3. The corpus holds the saved text, in vault order

`Corpus` holds `Option<Arc<str>>` slots indexed by the vault order that
`NoteTable` already assigns (`crates/index/src/notes.rs:64-71`). `None` is a
note not yet loaded.

- A save stores the `Arc<str>` that `NoteSaved` carries
  (`crates/app/src/vault/events.rs:11-15`), shared, not copied. A load
  converts its `String` once.
- A load or a save writes one slot through `Arc::make_mut`. The write copies
  pointers only if a search still holds the previous snapshot.
- **Corrected in slice 7 (smart quick open).** The slots live in chunks of 256
  behind `Arc`s, so that copy is the chunk list and one chunk, not every slot:
  2.3 ms became about 2 µs at 50,000 notes (smart quick open spec, Evolution,
  "Chunked corpus slots").
- Only `set_notes` reorders the slots, in one O(notes) pass.
- A snapshot is an `Arc` clone. No sort, no rebuild.

> **Corrected.** The draft rebuilt a sorted snapshot on the first query after
> a change. During the startup load every load is a change, and the rebuild
> measured 15 to 24 ms at 50,000 notes: a dropped frame per keystroke.

`Corpus::heap_bytes()` returns the sum of text lengths plus a fixed cost per
slot. It is maintained on each write, checked by a unit test, printed by the
bench, and logged when indexing ends. At 50,000 notes of 3 KB on average it is
about 150 MB. That is the price of D3, and the first thing the Evolution
levers reduce.

An inverted index (tantivy, SQLite FTS5) matches words, not substrings, and
adds a large dependency with its own invalidation. It stays a lever.

**Built in slice 4.** The slice 4 plan fixes these points D3 left open:

- The text write lands in slice 4, not slice 5. The `set_notes` and snapshot
  benches need a public slot write, and D1's only one is `insert` with the
  text. So `insert(id, source, text)` and `remove` clearing the text ship
  here, and `IndexSync::apply` passes the text: a load converts its `String`
  once, a save shares `NoteSaved::text`. The app holds the text from slice 4,
  about 156 MB at 50,000 generated notes, while its first reader, vault
  search, arrives in slice 10.
- `CorpusSnapshot` carries the ids beside the texts, since `Hit::note` needs
  each slot's `EntryId`. `NoteSet::build` computes them off the foreground.
  `set_notes` reads each id's previous slot from the previous `NoteTable`, in
  one pass, and returns the previous ids in the previous `NoteSet`.
- The texts of the notes gone move into the previous `NoteSet`, so the caller
  frees them off the foreground with it.
- `CorpusSnapshot` has `len`, `is_empty` and `heap_bytes`, as
  `NamesSnapshot` has `len` and `is_empty`. `Corpus` itself is crate-private.
- `heap_bytes` is slots × (`size_of::<Option<Arc<str>>>()` +
  `size_of::<EntryId>()`) plus, per loaded text, its length and its `Arc`
  header (the two counts, 16 bytes). A shared `Arc<str>` counts once per slot.

### D4. The text matcher

`TextQuery::new` NFC-normalizes the query and compiles the escaped literal
once per keystroke to a `regex::Regex`. It returns `None` for an empty query
or one made only of whitespace.

- **Case.** Insensitive by default, with Unicode simple case folding: `été`
  finds `Été`. The toggle makes it sensitive.
- **Whole word.** `\b` is added at each edge whose char is a word char:
  alphanumeric or `_`, which is regex's Unicode `\b`. It is decided on the
  first and last char of the unescaped query. Whole-word `été` matches in
  `l'été,` and not in `étés`. Never `(?-u:\b)`: it treats `é` as a non-word
  char.
- **NFD text.** The corpus stores text as saved. An NFC query does not match
  NFD text in v1. A test asserts it; the fix belongs with accent folding
  (Deferred).

`find_in` runs `find_iter` on one text and returns at most `limit` ranges.
`search` walks its range in vault order and calls `find_in` on each loaded
note, with a limit of 100 per note. It checks `cancel` between notes, and
stops once it holds `limit` hits, reporting `limit_reached`.

A hit's `context` is the hit extended within its line by up to 60 bytes
before and 100 after, cut on char boundaries. It is never shorter than the
hit. `context_match` is the hit's range inside `context`. Offsets are
`ByteOffset`, and never become a char offset.

The delegate clones the `TextQuery` once per range, since a `Regex` shared
across threads contends on its cache for many short haystacks (regex 1.13,
`src/lib.rs:531-547`).

`regex` becomes a direct dependency of `inkra-index`. It is already in
`Cargo.lock` at 1.13.1 through gpui, under MIT or Apache-2.0, which
`deny.toml` allows.

Limits: 1,000 hits in total, 100 per note. `Searched::merge` concatenates in
range order, which is vault order, and truncates. The order does not depend on
thread timing.

A whole-word query over French prose leaves regex's lazy DFA, which cannot
handle a Unicode word boundary on a non-ASCII haystack (regex-automata 0.4.18,
`hybrid/dfa.rs:243-254`). The bench below covers it. If it misses the budget,
`find_in` searches the plain literal and checks the edges in Rust, behind the
same API.

**Built in slice 4.** The slice 4 plan fixes these points D4 left open:

- A word char is regex's own, `regex_syntax::is_word_character`, not
  "alphanumeric or `_`". They differ: `½` is alphanumeric and not a regex word
  char, and a combining mark is the reverse. With Rust's rule, `\b` before `½`
  would demand a word char before it. `regex-syntax` 0.8 becomes a direct
  dependency; it was already in `Cargo.lock` through `regex`.
- The query is a literal: its spaces are kept, not trimmed.
- `limit_reached` means `hits.len() >= limit`, so it is true when exactly
  `limit` hits exist. `Searched::merge` concatenates, truncates and
  recomputes it, so any split equals one range.
- A set `cancel` returns `Searched::default()` at any limit, checked
  before the first note and between notes, as `match_names` returns an empty
  list.
- The query keeps its first 1,000 typed chars (`QUERY_MAX_CHARS`), so a huge
  paste never makes `regex` refuse it silently. Compiling 1,000 chars takes
  0.13 to 0.81 ms, the worst being an insensitive alternation of two forms.
  A query that `regex` still refuses is `None`, as an empty one is.
- A query that NFC changes matches in both forms. NFC maps singletons such as
  the Kelvin sign `K`, `Å` (U+212B) or `Ω` (U+2126) to another char, so a
  query pasted from a note would not find that note. When the typed query and
  its NFC form differ, `TextQuery` compiles an alternation of both escaped
  forms, each with its own whole-word edges. Both come from the same typed
  chars. A consequence: a decomposed (NFD) query now also finds NFD text. The
  NFD limitation above stays for an NFC query over decomposed text.
- `HITS_PER_NOTE` (100) is public, so vault search can show "100+" on a note
  whose hits reached the cap.
- A hit's context is found inside its window first: the newline search reads
  at most 60 bytes before and 100 after, never the whole line. On a 10 MB note
  of one line, a common word went from 50.9 ms to 0.016 ms (Performance).
- Whole word over a non-ASCII edge checks that edge in Rust. When whole word
  is on, the query has one form, and an edge that gets `\b` is a non-ASCII
  char, `find_in` searches the plain literal, which stays in regex's lazy DFA,
  and checks each edge as regex's Unicode `\b` does. After a rejected match it
  resumes one char after the match start, since a valid match can overlap a
  rejected one. Every other query keeps the escaped regex with `\b`. The v1
  regex is the test-only differential oracle over all four flag combinations.
  It gains 12 % on `é` and 13 to 68 % on Greek, Cyrillic and CJK, with no
  French or English case slower (Performance). An ASCII edge stays on the
  regex: the same check made whole word `e` slower, 494 to 510 ms against 337.

### D5. Keeping the index current: `LinkSync` becomes `IndexSync`

The links spec's reconcile loop stays the one feed (links spec, D6). It is
renamed `IndexSync`, in the same commit as `NoteIndex`.

- **Membership.** On each vault notify, `reconcile` clones the notes as
  `Vec<(EntryId, String, Kind)>` and runs `NoteSet::build` on the background
  executor. The result is applied on the foreground by `set_notes`, unless a
  newer build has started: a generation counter drops the stale one. Then the
  mtimes are diffed and the loads queued, as today.
- **Loads and saves.** Each load yields the `Source` and the text. Each
  `NoteSaved` yields both too. Both go through `NoteIndex::insert`.
- **Progress.** `IndexSync::progress() -> (loaded, total)`.

> **Corrected.** The draft kept `set_notes` on the foreground and left moving
> it to a trigger. That trigger is already met at 50,000 notes: the links spec
> measured 25 ms at 10,000 notes (Performance), about 125 ms at 50,000 if
> linear, on every rescan.

> **Corrected in slice 8.** The membership tuple is `(EntryId, String, Kind,
> Mtime)`: each corpus slot holds its text and that text's mtime (smart quick
> open spec, S3, "Built in slice 8").

During the startup load, vault search runs over the notes already loaded, and
says so (D9).

**Built in slice 2.** `note_set::prepare` filters, strips and normalizes the
notes once. `NoteTable` and `NamesSnapshot` share each path as one
`Arc<str>`. `insert` keeps its signature until slice 5 adds the text.
`generation` counts builds started and `built` names the one applied. A build
applies only while it is the latest. `apply_notes` frees the previous set off
the foreground. Freeing a 50k set took about 9 ms of the 10.53 ms first
measured; the swap alone takes 1.39 ms (Performance). `is_building()` is true
before the first build applies or while loads are queued or loading. A
rebuild that queues no load does not show "building"; a create shows it while
the new note loads. A load in flight for a note that leaves the vault stops
counting as pending. A stale build is freed off the foreground too.
`set_notes` prunes the gone sources in one pass per stem row (Performance).
`IndexSync` emits `NotesChanged` right after `set_notes` applies. The
backlinks panel rebuilds on it, not on the vault's notify. Its rows come from
the last applied build and their names from the vault. A load that notifies
inside the one-build window can show a renamed source's new name early. A row
whose source has no vault entry is skipped, so no row is ever nameless.
`IndexSync::revision()` is bumped on each change to the index; the panel skips
a notify whose revision and building state it has already seen, so one applied
build recomputes its rows once, not once for `NotesChanged` and again for the
notify after it. Vault search reuses this in slice 10. A save
can now reach `apply` before its note joins the index. `insert` then ignores
it, so `apply` forgets the note's mtime and the next diff reloads it. Until a
build applies, following a link resolves against the previous table; the
window is one build long, and accepted.

Two foreground turns remain per rescan. Measured at 50,000 notes, release, in
the slice 2 reviews: `reconcile` takes 1.4 to 4.9 ms by method (a criterion
bench of the path copy alone gave 3.33 ms; timed release tests of the whole
turn gave the rest); `apply_notes` with nothing changed takes 4.2 to 4.8 ms.
Both fit the 8 ms step budget. The Evolution table lists the levers.

**Built in slice 5.** Startup loads run in parallel, and the app logs a
startup timeline.

- `IndexSync` keeps one load per core in flight: `num_cpus()` ×
  `LOADS_PER_CORE`, with `LOADS_PER_CORE = 1`. Each load reads and parses at
  `Priority::Low`, as Zed's scanner does. Two loads per core measured the same
  (Performance, "Startup"), so one stays.
- A load counts until the foreground applies it, not until its read ends. At
  most one landed load per core waits in memory.
- One note has at most one load in flight. A queue entry for a note that is
  loading waits in the queue until that load lands, so a note rewritten in a
  loop cannot take every slot.
- Finished loads cross to the foreground over a channel. One foreground turn
  applies landed loads until 4 ms pass, then yields, so input and frames run
  between turns. A turn applies at least one load and never splits one. So
  the heaviest single note bounds a turn. That limit is accepted; the
  timeline's `longest turn` is its figure. A turn over 8 ms on a real vault
  is the trigger to split `insert`.
- A load for a note that left the vault keeps its slot until it lands, but
  it no longer counts as pending.
- A load or save acts only if its seq is the latest minted for its note, so
  a failed load cannot drop a note while a newer load or save is pending.
- A load that fails after a build applied, for a note still in the vault,
  queues a fresh load. A rename keeps the mtime, so no rescan reloads the
  note. The check is a build since the load started, not a changed path: a
  rename there and back ends at the path the load read. Builds come only from
  rescans, so the retries end.
- The release-only startup timeline is a gpui `Global` that `run()` sets. It
  logs one line once indexing has ended and the first frame is recorded,
  whichever comes last: `launch: scan …, first frame …, names ready …,
  indexed … (N notes, corpus B bytes, longest turn T)`. Each time counts
  from launch: the start of `run()`, not of the process. "first frame" is
  the first present, plus at most one frame interval. "longest turn" covers
  every `IndexSync` foreground turn: reconcile, build, load and save,
  observers included. A launch with no vault, or a run that draws no frame, logs no line.

### D6. The MRU

`AppShell` keeps the last 20 notes shown, newest first, in memory, updated in
`set_showing`. It passes them to `NameQuery::new`. A note no longer in the
vault is dropped when the list is read. Persistence is deferred.

A trashed note's id keeps its slot until newer notes push it off the end.
It is hidden when the list is read, so the list can show fewer than 20.

### D7. The picker

A new `app/src/picker/` module holds `Picker<D: PickerDelegate>`. The
delegate owns the query state and the matches, and renders a row. The picker
owns:

- a `NameField` for the query. `NameField` gains an `Edited` event, emitted by
  `edit_buffer` (`crates/app/src/field.rs:59-72`).
- a `uniform_list` of one-line rows with a scroll handle, as in the sidebar
  (`crates/app/src/sidebar/mod.rs:97-105`).
- the selected row. A delegate can mark rows as not selectable (vault-search
  headers); the selection skips them.

**Keys.** The deepest context wins in gpui (`gpui/src/keymap.rs:150-190`), and
the focused `NameField` already binds `enter` and `escape`
(`crates/app/src/keymap.rs:52-53`). So:

| Key | Context | Effect |
|---|---|---|
| `up`, `down` | `Picker` | move the selection |
| `enter` | `NameField` → `Confirmed` | the picker confirms the selected row |
| `escape` | `NameField` → `Cancelled` | the picker dismisses |
| `alt-cmd-c` | `VaultSearch` | toggle case sensitivity |
| `alt-cmd-w` | `VaultSearch` | toggle whole word |

The picker subscribes to `Confirmed` and `Cancelled`, as the sidebar does
(`crates/app/src/sidebar/edit.rs:152-155`). The picker's root adds the
delegate's key context, `QuickOpen` or `VaultSearch`, so the toggles exist only
in vault search. A later picker key must not collide with a `NameField`
binding. A click confirms a row.

> **Corrected.** The draft bound `enter` and `escape` in `Picker`. Those
> bindings could never fire under the field's.

**Query flow**, per keystroke:

1. `Edited` → `delegate.update_matches(query)`, which returns a `Task`.
2. The delegate sets the previous cancel flag, makes a new one, and bumps its
   search id.
3. If a search is still running, it waits 100 ms first.
4. It takes the snapshot from `NoteIndex` (an `Arc` clone), asks it for
   `chunks(cx.background_executor().num_cpus())`, and runs each range with
   `background_spawn`. Quick open uses one range in v1.
5. On the foreground, it drops the result if its id is stale. Else it merges,
   notifies, and selects the first selectable row.

The picker stores the `Task`, so replacing it cancels the one before. The
delegate sets its cancel flag on `Drop`, so a dismissed picker stops scanning
at once: dropping a `Task` does not interrupt a synchronous scan already
running.

**A changing vault.** Quick open subscribes to `IndexSync`'s `NotesChanged`,
since names change only when a build applies. On it, it re-runs the query
after 200 ms, as Zed's file finder does, and keeps the selected note if it is
still listed. It needs no `BUILDING_REFRESH`: loads do not change names. Vault
search observes `IndexSync`, since each load changes the corpus, and throttles
its refresh during the startup load by `BUILDING_REFRESH`, as the backlinks
panel does (`crates/app/src/backlinks/mod.rs`). Confirming a row whose note is
gone does nothing.

**Built in slice 3.** `PickerDelegate` takes no `Window`: no method needs one.
It has no `dismissed` hook: the delegate's `Drop` sets its cancel flag. The
delegate stores its results and the picker owns the selection. The delegate's
task ends with `Picker::matches_updated(select, cx)`, where `select` is `0` for
a new query or the kept note's row after a refresh; the picker snaps it to a
selectable row. The 200 ms refresh wait lives in `update_matches`, as Zed's
`debounce_next_refresh` (`file_finder.rs:1944-1954`). `NotesChanged` marks the
next run as a refresh and calls `Picker::refresh`, so a keystroke during the
wait supersedes it: one pending task, never two. `Edited` fires only when
`Buffer::version` changed, so a backspace at offset 0 runs no search. `up` and
`down` stop at the ends, as in the sidebar. Quick open runs one range per core
(`num_cpus`), not the one range of step 4 (Performance, slice 3). The stale-id
test calls the delegate directly: the picker drops the previous task, so only a
direct call reaches the id check. Highlight tests read the `(Range,
HighlightStyle)` runs a row is built from; the shots check the pixels.

**Confirm while a query is pending.** `enter` typed before the delegate's
result lands confirms that result, not the stale selection, as Zed's
`confirm_on_update` (`picker.rs:1034-1047`). `Picker::refresh` marks a result
pending and `matches_updated` clears it. An `enter` in between is stored and
confirms the row `matches_updated` selects: the first selectable row for a new
query, the kept note after a refresh. So `⌘P` `enter` typed at once opens the
previous note. A refresh reads the kept note when its result lands, not when
`NotesChanged` arrives, as Zed's `set_search_matches`
(`file_finder.rs:1117-1126`); a move during the 200 ms wait is kept.

> **Corrected in slice 8.** A run is a user search or a background refresh. A
> keystroke starts a user search, and so do the first names after the scan: the
> user waits on them. A `NotesChanged` refresh, a notify refresh and the re-run
> after a stale scan are background refreshes. Step 3 waits 100 ms only behind
> a user search: a keystroke during a background refresh cancels it and runs at
> once. A background refresh calls `Picker::refresh_in_background`, which marks
> no result pending, so `enter` during it confirms the row on screen at once.
> Its result lands through `Picker::matches_refreshed`: the kept note stays
> selected and the list keeps its scroll. When the kept note is gone, the first
> row is selected and scrolled to, through `matches_updated` (smart quick open
> spec, S5, "Built in slice 8").

**A refresh must not cancel a running scan of the same query.** Today a
`NotesChanged` bumps the search id and sets the cancel flag, so it drops the
scan of a query the user is still waiting on. A refresh, from `NotesChanged`
or, in slice 10, from observing `IndexSync`, either skips while a scan of the
same query runs, or marks the query dirty and re-runs it when that scan
completes. On a vault that keeps changing, cancelling would starve the scan.

### D8. The modal layer

`AppShell` gains `modal: Option<Modal>`, holding the picker view and the focus
handle that was focused when the first picker opened. It follows Zed's
`ModalLayer` (`workspace/src/modal_layer.rs`):

- Render adds a full-window child, `.absolute().inset_0().occlude()`, over the
  flex row. A left click on it dismisses. Inside, a 600 px container 15 % from
  the top, also `.occlude()`, stops propagation on mouse down. gpui's hit test
  stops only at an occluding hitbox (`gpui/src/window.rs:1106-1123`), so
  without both, a click on a row also reaches the editor below.
- Opening a picker replaces the other one and keeps the stored handle.
  Pressing the key of the picker showing dismisses it.
- On dismiss, the stored handle gets focus back if the modal still holds focus,
  else the shell's handle. The picker also dismisses when focus leaves it.
- `cmd-p` during a sidebar rename moves focus away from the rename field. The
  sidebar keeps the field open, as it does today for any focus change; the
  rename is not cancelled.
- The pickers do not open when no vault is open.
- Accessibility: the modal has a dialog role and an `aria_label` ("Quick
  open" or "Search notes"), the list a list role, and each row an
  `aria_label` with its plain text (`gpui/src/elements/div.rs:1304`, `:1326`).
  The selected row is not announced until `set_active_descendant` is wired;
  `docs/gpui-accessibility.md` records it.

> **Corrected.** The draft had no occlusion and returned focus to the shell,
> which is the editor whenever one exists. From the tree, `cmd-p` then
> `escape` would have moved focus to the editor.

**Built in slice 3.** The delegate gives the two labels: `label()` names the
dialog and `row_label(ix)` gives a row's plain text. The picker dismisses on
blur only while the window is active, as Zed (`picker.rs:1197`, `:1208`), so
switching to another app keeps it open. Confirming the note showing does not
reopen it, as from the backlinks panel: reopening would reset its caret and
scroll. From the picker or the panel, it still focuses the editor, as another note does.
gpui has `.aria_active_descendant()`, but it needs the focused node to
be an ancestor of the row, and the focused query field is not; see
`docs/gpui-accessibility.md`.

**A load landing under the picker.** An activation that completes while a
modal is open does not move focus to the editor. The editor's handle becomes
the modal's stored handle instead, so the picker keeps its query and its keys,
and dismissing it lands on the note.

**Picking the note showing supersedes a pending load.** From quick open or the
backlinks panel, confirming the note showing does not reopen it, and it drops
the pending activation of any other note: the pick is the user's last word.

**Focus after the note showing closes.** `NoteClosed` moves focus to the
shell's handle when the dropped editor held it; a dropped view's handle
reaches no action. On dismiss, a stored handle not drawn in the last frame
falls back to the shell's handle, as the one of a trashed note's editor.

A row confirms on click, not on mouse down, as Zed's.

Bindings, in the `AppShell` context so they fire from any focus, as `cmd-n`
does:

| Key | Action |
|---|---|
| `cmd-p` | `QuickOpen` |
| `cmd-shift-f` | `SearchVault` |

None of `cmd-p`, `cmd-shift-f`, `alt-cmd-c` or `alt-cmd-w` is bound today
(`crates/app/src/keymap.rs`).

### D9. Rows and states

**Quick open** rows show the stem with its matched chars highlighted, then the
folder path dimmed, with its own highlights. A long folder path is cut from
its start (`…/projects/2026`), so the nearest folder stays visible.

**Vault search** rows follow the Text Finder. One non-selectable header row
per note shows its name and hit count. One row per hit follows it, showing the
hit's `context` with `context_match` highlighted.

Highlights use `StyledText::with_highlights`, as the backlinks panel does
(`crates/app/src/backlinks/render.rs:29-38`). Colours come from `theme.rs`.
Every row is one line, so `uniform_list` can measure from row 0.

> **Corrected in slice 8.** Quick open's rows are two lines: the name, then an
> excerpt or the note's first line (smart quick open spec, S4, S5). Every row of
> a picker still has one height, the delegate's `row_height`.

The field's trailer shows the two toggles, `Aa` and `W`, each lit when on.

| State | Quick open | Vault search |
|---|---|---|
| Empty query | the MRU; placeholder "Open a note…" | nothing; placeholder "Search notes…" |
| Empty MRU | "No recent notes" | — |
| No match | "No matching notes" | "No matches" |
| No match while indexing | — | "No matches in 12,340 of 50,000 notes indexed so far" |
| Indexing, with matches | — | footer "Indexing 12,340 of 50,000 notes" |
| `limit_reached` | — | footer "1,000+ results; refine the query" |

Each state is rendered in a shot scene and looked at.

**Built in slice 3.** At most 10 rows show; the list scrolls past them, and its
height is `rows × UI_LINE_HEIGHT`. The placeholder is an overlay the picker
draws over the field while the query is empty; `NameField` has no placeholder.
A long folder is cut by width with gpui's `text_ellipsis_start`
(`gpui/src/styled.rs:97`). The end stays visible, but the cut can fall inside
a folder name rather than at a `/`. A row is cut from the match's NFC path,
which `positions` index, and a highlight covers the whole grapheme the matched
char lies in: `é` is one grapheme whether precomposed or not, and an emoji ZWJ
sequence lights whole. The shot scenes `quick-open-results`, `quick-open-empty`,
`quick-open-no-match` and `quick-open-graphemes` render the quick-open states.

A file name may hold a newline on macOS. A row, and its `aria_label`, draws
each ASCII control char as a space: one byte for one, so the highlight ranges
stay valid. The sidebar tree row does the same. A stem wider than the row
shrinks with an end ellipsis; the folder's flex basis is 0, so it gives up
its width first.

### D10. Opening a hit at the match

Quick open emits `NoteActivated(id)`, the event the sidebar and backlinks
already emit. The shell opens the note as it does today.

Vault search emits `HitActivated { note, range }`. The shell passes the range
through `open_note(id, Option<Range<ByteOffset>>)`:

- Inside the activation update, right after `open_buffer`, the editor sets the
  selection to the range, clipped by `Buffer::clip`, through its `move_cursor`
  gate (`crates/app/src/editor/mod.rs:188-205`). The note may be open with
  unsaved edits, so the saved offsets may not fit the live text. `clip` snaps
  to a char boundary, not a grapheme boundary; the selection may split a
  grapheme, and the next caret move corrects it.
- A superseded activation is a dropped `Task`, so it cannot apply a stale
  range.
- Only the reveal is deferred. `reveal_cursor` needs `last_text_top`, which is
  written in paint (`crates/app/src/editor/element.rs:210-216`), and
  `open_buffer` clears it. `EditorView` gains a `pending_reveal` flag,
  consumed after the first paint with a non-zero viewport. It calls
  `reveal_cursor` and notifies. The note shows at its top for one frame, then
  scrolls. Zed reveals in prepaint instead; recorded here as the alternative
  if the jump is noticed.
- If the hit is in the note showing, the shell selects and reveals without
  rebinding, which would reset the caret and scroll, as `follow` already
  avoids (`crates/app/src/shell.rs`).

> **Corrected.** The draft deferred the selection to the next layout. The
> selection is an input to shaping, since conceals reveal around it, so the
> first layout would have been shaped with the caret at 0.

This also gives the links spec its deferred "open at offset" (links spec,
Deferred, "Scroll to `#heading`…"). Backlinks do not adopt it in this spec.

## Performance and the benches

Two criterion benches join `crates/index/benches/`. Each figure is recorded in
this spec with the machine and its core count. A lever is measured with
`cargo bench -p inkra-index -- --save-baseline before`, then
`--baseline before`, on one machine. Single-thread throughput in bytes per
second (`Throughput::Bytes`) is the figure that travels between machines.

- **`names`**: 50,000 paths, 3 to 5 components, words from a fixed
  vocabulary with accented ones. Cases:
  - `NoteSet::build` at 50,000 notes (background, no budget; recorded).
  - `set_notes` with a built `NoteSet`, including the corpus reorder. Budget
    8 ms, the foreground limit.
  - `match_names` on one core, queries of 1, 3 and 8 chars, two terms, and an
    unaccented query over accented names. Budget 16 ms each.

  Measured, slice 1, on 2026-09-24: Apple M1 Pro (MacBookPro18,1), 10 cores,
  one core per case, `cargo bench -p inkra-index --bench names`, criterion's
  middle estimate. "First" is the plan's code; "after review" adds the
  red and blue team fixes (ASCII fast paths, the stem-pass skip, the query
  cap and repeat drop). Three vaults of 50,000 notes:

  - **base**: 3 to 5 components, 2 to 4 folders then a stem `<word> <n>`,
    about 36 bytes; two thirds hold an accented word. Scanned through `MemFs`.
  - **long stems**: 2 or 3 folders, then a stem such as `Meeting with Zoé
    about the roadmap 12`, about 56 chars.
  - **deep**: 7 to 9 components, about 60 chars.

  | Vault | Case | First | After review | Budget 16 ms |
  |---|---|---:|---:|---|
  | base | `NoteSet::build` | 65.13 ms | 41.35 ms | none, background |
  | base | 1 char `p` | 3.18 ms | 2.94 ms | met |
  | base | 3 chars `pln` | 3.33 ms | 2.76 ms | met |
  | base | 8 chars `projplan` | 0.81 ms | 0.71 ms | met |
  | base | two terms `plan jour` | 1.31 ms | 1.12 ms | met |
  | base | unaccented `ete` | 11.52 ms | 9.77 ms | met |
  | base | `e e e e e e e e` | — | 6.49 ms | met |
  | base | `"e ".repeat(150)` | — | 6.47 ms | met |
  | base | 300 chars of distinct 2-char terms | — | 3.68 ms | met |
  | base | empty query, 20-note MRU | — | 0.37 ms | met |
  | long stems | `ete` | — | 31.21 ms | **missed** |
  | long stems | `pln` | — | 9.70 ms | met |
  | long stems | two terms `plan jour` | — | 3.87 ms | met |
  | long stems | `aeiou` | — | 23.37 ms | **missed** |
  | deep | `ete` | — | 24.16 ms | **missed** |
  | deep | `pln` | — | 9.38 ms | met |
  | deep | two terms `plan jour` | — | 7.48 ms | met |
  | deep | `aeiou` | — | 12.93 ms | met |

  The base vault meets every budget on one core. Realistic names do not: a
  short, frequent query over long stems or deep paths takes up to 31 ms. So
  "Parallel quick open" ships in slice 3. At 31 ms on one core it needs about
  two cores to fit 16 ms.

  Measured, slice 2, on 2026-09-24, same machine and method, base vault:

  | Case | Figure | Budget |
  |---|---:|---|
  | `NoteSet::build`, 50k, now with the `NoteTable` | 104.84 ms (was 41.35 ms) | none, background |
  | `set_notes`, 50k, a built `NoteSet`, one source per note, before the change | 10.53 ms | 8 ms, **missed** |
  | of which: dropping the previous `NoteSet` (diagnostic, not kept) | 8.86 ms | — |
  | `set_notes`, 50k, foreground part, the previous set returned | 1.39 ms | 8 ms, met |
  | `set_notes`, 10k notes, 200k links (`index` bench), before the change | 1.23 ms | none |
  | `set_notes`, 10k notes, 200k links (`index` bench), the previous set returned | 0.11 ms | none |

  The swap and the pruning take about 1.4 ms. Freeing the previous table and
  names took the rest. The lever is applied: `set_notes` returns the previous
  `NoteSet`, and `IndexSync` drops it on the background executor.

  Pruning, a linked vault: note n links `[[note n+1]] [[index]]`, so every
  source's `[[index]]` link shares one `by_stem` key, the `index` row. The
  targets do not resolve in this vault, so only the row's length matters.
  Only `set_notes` is timed. The first pruning removed each gone note on its
  own, one pass over the `index` row per note. It now removes all gone
  sources, then passes once over each row they touched.

  | Case | Per-note pruning | One pass per row | Budget 8 ms |
  |---|---:|---:|---|
  | nothing gone | 0.85 ms | 0.78 ms | met |
  | the first 500 notes gone | 11.67 ms | 1.52 ms | met |
  | the first 5,500 notes gone | 107.83 ms | 3.82 ms | met |

  Measured, slice 3, on 2026-09-24, same machine (Apple M1 Pro, 10 cores),
  `cargo bench -p inkra-index --bench names -- parallel`, criterion's middle
  estimate. The `parallel` group runs the quick-open delegate's split on 10
  threads with `std::thread::scope`: `chunks(10)`, `match_names` per range,
  then `merge_names`. gpui's test executor is simulated and single-threaded, so
  no headless test times the real split.

  | Vault | Query | One core (slice 1) | 10 threads | Budget 16 ms |
  |---|---|---:|---:|---|
  | long stems | `ete` | 31.21 ms | 9.02 ms | met |
  | long stems | `aeiou` | 23.37 ms | 6.28 ms | met |
  | long stems | `pln` | 9.70 ms | 2.91 ms | met |
  | deep | `ete` | 24.16 ms | 6.38 ms | met |
  | deep | `aeiou` | 12.93 ms | 3.72 ms | met |
  | deep | `pln` | 9.38 ms | 3.06 ms | met |

  Every case meets 16 ms, so "Sparse DP" stays untriggered. The speed-up is
  3.1 to 3.8 times on 10 threads, not 10. Thread start-up, the serial merge
  and the two efficiency cores probably account for the rest; not profiled.
- **`search`**: 50,000 notes of 1 to 5 KB of generated French and English
  prose, about 150 MB. Cases, each single-thread over the full corpus, in
  bytes per second:
  - a rare word, a common word, a 1-char query that hits the cap;
  - a frequent prefix with no match (`projetzz`), the full scan with no early
    stop;
  - whole-word, a frequent 1-char substring with no whole-word hit (`é`);
  - whole-word, a common word.
  - `Corpus::heap_bytes()` is printed.

  The budget per core follows from the requirement. 150 MB in 100 ms on 4
  cores needs about 375 MB/s per core. Measured in review with regex 1.13.1,
  single thread: a rare word in 17 ms, `(?i)projetzz` in 95 to 130 ms, and
  `(?i)\be\b` in 310 to 339 ms with no hit. The last one misses the budget on
  4 cores, so D4's fallback is likely needed. The bench decides.
- **Snapshot**: `corpus()` and a slot write while a snapshot is held, at
  50,000 notes. Budget 2 ms.

  Measured, slice 4, on 2026-09-25: Apple M1 Pro, 10 cores, one thread per
  case, `cargo bench -p inkra-index --bench search` and `--bench names`,
  criterion's middle estimate, regex 1.13.1. The corpus is 50,000 generated
  notes of 1 to 5 KB of French and English prose: 155,445,432 bytes of text,
  `heap_bytes()` 156,445,432. Every case runs case-insensitive, limit 1,000.
  MB/s is 155.4 MB over the time; a case that stops at the cap reads less,
  so it has no MB/s. The per-core budget is 375 MB/s.

  | Case | Figure | MB/s | Budget |
  |---|---:|---:|---|
  | rare word `zanzibar`, full scan | 20.22 ms | 7,690 | met |
  | common word `projet`, stops at 1,000 | 0.19 ms | — | met |
  | 1 char `e`, stops at 1,000 | 0.09 ms | — | met |
  | `projetzz`, no match, full scan | 115.47 ms | 1,346 | met |
  | whole word `é`, no hit, full scan | 280.91 ms | 553 | met |
  | whole word `e`, no hit, full scan | 334.05 ms | 465 | met |
  | whole word `zanzibar`, full scan | 19.38 ms | 8,020 | met |
  | whole word `réunion`, stops at 1,000 | 0.38 ms | — | met |
  | `corpus()` | 11 ns | — | 2 ms, met |
  | a slot write (`insert`), a snapshot held | 0.16 ms | — | 2 ms, met |
  | `set_notes`, 50k, empty text in every slot | 2.99 ms | — | 8 ms, met |
  | `set_notes`, 50k, text in every slot | 1.89 ms | — | 8 ms, met |
  | `set_notes`, 50k, text in every slot, a snapshot held | 2.09 ms | — | 8 ms, met |

  Parallel, measured by the slice 4 red and blue teams on the same machine:
  `chunks(n)`, `search` per range on `std::thread::scope` threads, then
  `Searched::merge`, as slice 3 measured quick open. The budget is 100 ms.

  | Case | 4 threads | 10 threads | Budget 100 ms |
  |---|---:|---:|---|
  | whole word `e`, no hit | 88 to 94 ms | 51 to 60 ms | met, thin on 4 |
  | whole word `é`, no hit | 79 ms | 45 ms | met |
  | `projetzz`, no match | 31 ms | 22 ms | met |

  On 10 threads, more ranges than threads (2 to 4 × `num_cpus`) finished
  faster than one range per thread, since ranges of equal note counts carry
  unequal bytes.

  Scripts, the `scripts, 10 MB, lines of 80 bytes` group: one 10 MB note of
  Greek, Cyrillic or CJK prose, whole word, no hit. "Before" is the escaped
  regex with `\b`; "after" checks a non-ASCII edge in Rust (D4). The French
  and English cases of the `search` group moved by −3.5 % to +0.8 %: whole
  word `é` went from 281.26 to 248.53 ms (553 to 625 MB/s) and `é ` from 89.85
  to 79.26 ms; whole word `e`, an ASCII edge, stayed at 332 ms.

  | Case | Before | After | After, MB/s | Budget 375 MB/s |
  |---|---:|---:|---:|---|
  | Greek `σ` | 52.90 ms | 34.87 ms | 301 | **missed** |
  | Greek `α` | 31.93 ms | 23.67 ms | 443 | met |
  | Greek `ο` | 29.39 ms | 21.29 ms | 493 | met |
  | Cyrillic `е` | 29.08 ms | 25.24 ms | 415 | met |
  | Cyrillic `о` | 36.73 ms | 27.64 ms | 379 | met, at the edge |
  | Cyrillic `ет` | 13.25 ms | 11.22 ms | 934 | met |
  | CJK `的` | 7.19 ms | 5.03 ms | 2,085 | met |
  | CJK `项` | 23.17 ms | 7.51 ms | 1,396 | met |
  | CJK `会` | 22.94 ms | 7.50 ms | 1,398 | met |

  Greek `σ` over Greek prose, a letter in nearly every word, stays under the
  per-core budget. At 301 MB/s, 150 MB takes 125 ms on 4 cores and about 60 ms
  on 10. The lever is the bigram prefilter (Evolution).

  Long lines, the `long lines` group: one note of 10 MB, or ten of 1 MB, with
  a line break every 80 bytes or none. The first context builder scanned the
  whole line for its newlines; it now reads only its window (D4).

  | Case | Before | After |
  |---|---:|---:|
  | one 10 MB note, one line, `projet` | 50.94 ms | 0.016 ms |
  | ten 1 MB notes, one line each, `projet` | 48.50 ms | 0.16 ms |
  | one 10 MB note, lines of 80 bytes, `projet` | 0.014 ms | 0.015 ms |
  | any of them, `zanzibar` (rare, a full scan) | 1.13 to 1.16 ms | unchanged |

  Mutants, slice 4, `cargo mutants` in a scratch worktree: the slice diff of
  `crates/index/src` had 51 mutants, 32 caught, 2 missed, 17 unviable. The
  two misses were on the slot cost and are now pinned by a test; `corpus.rs`
  re-run: 34, 23 caught, 0 missed. `index.rs` and `note_set.rs`: 23, 16
  caught, 0 missed. `crates/app/src/index.rs`: 53, 51 caught, 0 missed.
  After the red and blue fixes, the diff of `crates/index/src` since those
  runs: 46 mutants, 37 caught, 0 missed, 7 unviable, 2 timeouts. Both
  timeouts turn the resume step of the whole-word check into an endless
  loop, so a test does catch them.

**Startup** cannot be measured headless: gpui's test scheduler is simulated
and `MemFs` has no I/O cost. `cargo run -p inkra-app --release --example
startup_vault` writes 50,000 notes to `$TMPDIR/inkra-startup-50k` and prints
the path. It writes them again only when its parameters change. The release
app logs the time until the queue is empty and `heap_bytes()` when indexing
ends. That `launch:` line, on this vault, is the figure recorded here.

  Measured, slice 5, before parallel loads. The machine is an Apple M5 Pro
  with 18 cores (`sysctl -n machdep.cpu.brand_string hw.ncpu`). The vault is
  the one `startup_vault` writes: 50,000 notes of 1 to 5 KB in 100 folders,
  in `$TMPDIR/inkra-startup-50k`, from the same generator as
  `crates/index/benches/search.rs`.

  - Release app, before parallel loads (`9c4ae9b`), taken by the
    orchestrator with `./target/release/inkra-app "$TMPDIR/inkra-startup-50k"`:
    - Cache partly warm, right after the `#[ignore]` runs: `launch: scan
      279.7ms, first frame 514.3ms, names ready 416.5ms, indexed 10.2s
      (50000 notes, corpus 157245432 bytes, longest turn 5.0ms)`.
    - Cache warm: `launch: scan 264.2ms, first frame 488.6ms, names ready
      387.9ms, indexed 5.5s (…, longest turn 4.7ms)`.

  History, not a figure. Until review of slice 5, an `#[ignore]` test,
  `startup_figure_on_real_fs`, generated the vault and ran `IndexSync` over
  `RealFs`. It was removed because no app test touches the disk. Its
  generator became the `startup_vault` example. Its serial figures stay here
  for comparison only:

  - Second run:
    `cargo test -p inkra-app --release -- --ignored startup_figure_on_real_fs --nocapture`.
    The scan takes 215.7 ms. Indexing ends at 1.9 s. The corpus holds
    157,245,432 bytes. The test scheduler runs every load on one thread.
  - Its timeline line: `launch: scan -, first frame -, names ready 238.4ms,
    indexed 1.9s (50000 notes, corpus 157245432 bytes, longest turn 4.8ms)`.
    The test takes no scan mark and draws no frame.
  - Cache partly cold (the first run right after a release build): scan
    379.7 ms, indexed 7.5 s, longest turn 4.0 ms.

  Caveats on the `#[ignore]` figure:

  - It is the second run, so the page cache is warm.
  - The test scheduler polls background and foreground tasks on one thread.
    Each serial load's round trip to the main thread costs nothing there,
    unlike in the app.
  - The test has no window and no observers, so `longest turn` excludes
    observer work.
  - The handoff's 11.25 s came from a different run whose vault is not
    recorded.

  So only the release app's line compares with 11.25 s and with Task 5.3.

  Measured, slice 5, after parallel loads and the review fixes (`a269565`),
  same machine and vault. `longest turn` now counts every `IndexSync`
  foreground turn: reconcile, build, load and save. The release app ran in
  the background until its `launch:` line, then was killed.

  | Run | scan | first frame | names ready | indexed | longest turn |
  |---|---:|---:|---:|---:|---:|
  | first after the build, cache partly cold | 636.4 ms | 862.8 ms | 808.2 ms | 1.9 s | 6.3 ms |
  | warm | 266.9 ms | 397.1 ms | 386.2 ms | 1.2 s | 5.9 ms |
  | warm | 265.9 ms | 386.7 ms | 380.0 ms | 1.2 s | 5.8 ms |

  - Indexing ends at 1.2 s warm, against 5.5 s warm before: about four
    times faster. The main thread no longer pays one round trip per load.
  - No turn goes over 6.3 ms, under the 8 ms step budget but over the 4 ms
    aim. Before reconcile's own turn was counted, the longest turn measured
    4.0 to 4.6 ms, so the extra is probably reconcile's copy of 50,000 vault
    entries (D5, 1.4 to 4.9 ms in slice 2). The lever is "Vault entries as
    `Arc<[Entry]>`" in the Evolution table.
  - History: at `121f04e`, before reconcile and save turns were counted,
    two loads per core measured the same as one (indexed 1.2 to 1.5 s), so
    `LOADS_PER_CORE` stays 1. The serial `#[ignore]` test gave indexed
    2.0 s.
  - The first frame still takes 387 to 863 ms, over the 100 ms budget: the
    window waits on the scan. Slice 6, "Open first, scan behind", takes that
    on.

**End to end**, a release-only log in each delegate records the time from
`Edited` to `notify`. It is checked once on the generated vault.
The quick-open log is built in slice 3 (`quick open: N matches, T from the
edit`). It has not been read on a 50,000-note vault on disk yet: that needs a
person typing in a release build, so slice 3 claims no end-to-end figure.

## Testing

- **Oracles in `inkra-index`.** Proptest compares each matcher with a naive
  model across random vaults and queries: NFD, mixed case, accents, and the
  fold-special chars `K` (Kelvin sign), `ſ` and `É`.
  - `match_names` returns the same set of notes as "every term is a
    subsequence of the folded path, by D2's char rule". Every position is in
    bounds and on a char boundary. With `showing` and `recent` generated,
    `merge_names` over any split equals one range.
  - `search`, ASCII queries over ASCII text: the same hits in the same order
    as `str::match_indices` over ASCII-lowercased text. Unicode folding makes
    `k` match the Kelvin sign, so this model stops at ASCII.
  - `search`, all inputs: each hit is on char boundaries, and `context`
    contains the match at `context_match`. A query longer than 160 bytes is
    one case.
  - `Searched::merge` over any split equals one range.
  - Whole word: `été` matches in `l'été,`, not in `étés`; a query starting
    with punctuation.
  - An NFC query over NFD text finds nothing (D4).
- **The differential oracle.** The v1 `find_in` (escaped regex, no prefilter)
  is kept as a test-only reference once a lever replaces it. Every later
  `search` must return identical hits in identical order, for all four flag
  combinations, over Unicode text. This, with the benches, is the contract
  for every lever.
- **Cancellation.** A set flag makes each function return early.
- **`NoteIndex`**: a `heap_bytes` test; an `insert` after a `set_notes` that
  removed the note leaves no links and no text.
- **`IndexSync`**, against `MemFs`, extending the `LinkSync` tests: a load, a
  save, a rename, a trash and a rescan each update the corpus as they update
  links; a load finishing after a trash leaves nothing; a stale `NoteSet`
  build is dropped.
- **The picker**, headless:
  - `cmd-p` and `cmd-shift-f` open it; each picker's own key dismisses it.
  - `up`, `down`, `enter`, `escape`, `alt-cmd-c` and `alt-cmd-w` each do what
    D7 says, with a `simulate_keystrokes` test each.
  - `up` and `down` skip headers.
  - A stale result is dropped.
  - With `num_cpus` overridden, the merged result equals one range.
  - A click on a row leaves the editor's selection unchanged. A click on the
    backdrop over a sidebar row dismisses, and does not activate the row.
  - From the tree, `cmd-p` then `escape` leaves the tree focused.
  - Trashing a listed note removes its row.
  - A hit far down a long note opens with the range selected and visible.
- **Shots**: quick open with results, vault search with headers and toggles,
  and each state of D9's table.
- **Mutants**: `cargo mutants` on `inkra-index` after slices 1, 2, 4 and 5.

## Slices

Each slice merges alone with `check.sh` green, and records its bench figures.

1. **`names`**: `NoteSet::build` (names only), `NameQuery`, `match_names`,
   `merge_names`, `chunks`, the char rule, the oracles, the `names` bench.
2. **`NoteIndex` and `IndexSync`**: the two renames in one commit, then
   `NoteSet` holding `NoteTable`, the background build with its generation
   counter, `names()`, `progress()`. The `set_notes` budget is checked.
3. **Quick open**: `NameField::Edited`, `Picker`, the modal layer, the
   quick-open delegate, the MRU, `cmd-p`, the key and focus tests, the
   accessibility roles, the shots.
4. **`search`**: `Corpus`, `TextQuery`, `find_in`, `search`,
   `Searched::merge`, `heap_bytes`, the oracles, the `search` and snapshot
   benches. `regex` checked against `deny.toml`. The whole-word fallback lands
   here if the bench needs it.
5. **The corpus feed**: `NoteIndex::insert` takes the text, `IndexSync` passes
   it on load and save. The startup figure is recorded.
6. **Vault search**: the delegate, headers, toggles, `cmd-shift-f`,
   `HitActivated`, `open_note` with a range, `pending_reveal`, the key tests,
   the shots.

## Evolution

Each lever keeps the D1 API unless stated, passes the oracles and the
differential oracle, and is justified by a bench figure before and after.

| Lever | Trigger | What changes |
|---|---|---|
| Bigram prefilter (fff) | A `search` case over its per-core budget, or a slow real vault | `Corpus` keeps a bitset per bigram of case-folded text; `search` skips notes that lack one. About 30 MB at 50,000 notes, per fff |
| Whole-word check in Rust | Shipped in slice 4 for a non-ASCII edge (D4): `é` −12 %, Greek, Cyrillic, CJK −13 to −68 %. Measured and rejected for an ASCII edge: the plain literal plus Rust edges took 494 to 510 ms on whole word `e` against 337. A `(?-u:\b)` candidate pass with a Unicode recheck is sound only for ASCII edges: `e` 306 against 341 ms, `réunion` 75 against 189 ms. Trigger: an ASCII-edge whole-word case over budget | Inside `find_in` (D4) |
| `memchr::memmem` fast path | A profile shows `regex` in the case-sensitive path. regex already runs a single literal through its own `memchr`-based prefilter, so the gain is probably small | Inside `find_in` |
| Balanced chunks | Ranges finish far apart in the end-to-end log | `chunks` splits by bytes, not by count |
| Parallel quick open | Shipped in slice 3: `ete` over 50,000 long stems takes 31 ms on one core and 9.02 ms on 10 threads (Performance, slice 3) | `app` passes `num_cpus` to `NamesSnapshot::chunks`, runs one range per core, and calls `merge_names` |
| Sparse DP | `names` over 16 ms per core after parallel quick open | `fill` visits only the columns where the query char matches. The dense `fill` stays as a test-only differential oracle |
| Compact candidates | A names memory figure over ~50 MB on a real vault | A candidate stores `exact` only, with a sparse `base` for the accented chars, and merges `chars` and `bonus` into one allocation. About 22 to 37 MB at 50,000 notes today |
| Streaming results | First results over 100 ms on a real vault | The delegate appends each range's hits in vault order once every earlier range is done, as the Text Finder does; keeps the selection across batches |
| Compressed text (e.g. lz4) | `heap_bytes` over 300 MB on a real vault | `Corpus` stores compressed blocks; `search` decompresses candidates. Best after the prefilter |
| No resident text | Memory still too high after compression | `Corpus` keeps the prefilter only. **Changes the API**: `CorpusSnapshot::candidates(range, &TextQuery)` lists notes, `app` loads them through `Fs` and calls `find_in`. Needs the prefilter first, as fff shows |
| Parallel startup loads | Shipped in slice 5: the release app went from 5.5–10.2 s to 1.2–1.6 s at 50,000 notes (Performance, "Startup") | `IndexSync` keeps one load per core in flight, one read per note, and applies landed loads until 4 ms pass, at least one per turn |
| `next_load` scans the queue | A foreground turn over 8 ms whose profile shows `next_load` | A set of the notes in flight for `next_load`'s check, and held-back entries parked beside the queue until their load lands, instead of rescanning them on each call. Today at most one entry per load in flight is held back, so a call costs O(cores²) |
| Vault entries as `Arc<[Entry]>` | A file watcher lands, or a foreground step over 8 ms | `Vault` shares its entries; `reconcile` becomes an `Arc` clone, and the build returns the mtime list for the diff |
| Cheaper `Key` in `NoteTable::new` | A file watcher lands, or rebuild latency is noticed | Skip the redundant NFC, derive the stem key as a suffix of the path key; needs a proptest against today's keys. About 106 → 80 ms per build |
| Coalesced rebuilds | A file watcher lands (the red team noted starvation under a stream of notifies) | A notify during a build sets a dirty flag instead of starting a second build; the build restarts once it applies |
| `apply_notes` liveness via `NoteIndex` | The `apply_notes` turn over 5 ms after slice 4 | Drop the 50k `HashSet` of live ids; ask the index for membership |
| Persisted index | Cold start too slow even with parallel loads | **Changes the API**: `index` serializes the prefilter to bytes, `app` writes them through `Fs` and invalidates by mtime |

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Persisted frecency | The MRU forgets across restarts and a user misses it | fff's decay (`dbs/frecency.rs:373-418`), stored with the vault state |
| Typo tolerance | A user misses a note by a typo | `frizbee` 0.13 (MIT) or a bounded edit distance per term |
| Regex queries | The user asks | `TextQuery` already compiles to a regex; the question is UI and errors |
| `path:`, `tag:`, `-exclude` | The user asks | A small query parser in front of `TextQuery` |
| Accent folding and NFD in note text (`ete` finds `été`) | The user asks | Folds text and query; hit offsets need a map back to the saved text |
| Relevance ranking of content hits | Vault order is not useful | Notes whose name matches first is the cheap step |
| Find in note (`⌘F`) | The user asks | `find_in` over the buffer; Zed splits the buffer into chunks (`editor/src/items.rs:2010-2087`) |
| Search in unsaved buffers | Two seconds of lag is observed | Links spec, D5; `find_in` over open buffers |
| Unlinked mentions | The user asks | A `search` for each note's name |
| `[[` completion | Links spec, Deferred | Reuses `match_names` |
| Backlinks open at their line | Links spec, Deferred | Reuses `open_note` with a range |
| Query and selected-row announcement | A screen-reader user asks | Two conflicting fixes, decided by a manual screen-reader probe; see `docs/gpui-accessibility.md` |
